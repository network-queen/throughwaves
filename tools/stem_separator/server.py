"""
JamHub Stem Separator Service
Uses Meta's Demucs model to separate audio into stems (vocals, drums, bass, other).
Supports file uploads and URL downloads via yt-dlp.
"""

import os
import sys
import uuid
import shutil
import subprocess
import threading
import time
from pathlib import Path
from typing import Optional

from fastapi import FastAPI, UploadFile, File, HTTPException
from fastapi.responses import FileResponse, JSONResponse
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel

app = FastAPI(title="JamHub Stem Separator", version="1.0.0")

# Allow CORS from the DAW and web frontend
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Directory for all job data
JOBS_DIR = Path("jobs")
JOBS_DIR.mkdir(exist_ok=True)

STEM_NAMES = ["vocals", "drums", "bass", "other"]


class JobStatus:
    PENDING = "pending"
    DOWNLOADING = "downloading"
    SEPARATING = "separating"
    COMPLETE = "complete"
    FAILED = "failed"


# In-memory job registry
jobs: dict[str, dict] = {}


class UrlRequest(BaseModel):
    url: str


def _create_job() -> str:
    """Create a new job and return its ID."""
    job_id = str(uuid.uuid4())
    job_dir = JOBS_DIR / job_id
    job_dir.mkdir(parents=True, exist_ok=True)
    jobs[job_id] = {
        "status": JobStatus.PENDING,
        "progress": 0.0,
        "message": "Queued",
        "stems": {},
        "error": None,
        "created_at": time.time(),
    }
    return job_id


def _download_url(job_id: str, url: str) -> Optional[Path]:
    """Download audio from URL using yt-dlp. Returns path to audio file."""
    job = jobs[job_id]
    job["status"] = JobStatus.DOWNLOADING
    job["progress"] = 0.05
    job["message"] = "Downloading audio from URL..."

    job_dir = JOBS_DIR / job_id
    output_path = job_dir / "input.%(ext)s"

    try:
        result = subprocess.run(
            [
                "yt-dlp",
                "--extract-audio",
                "--audio-format", "wav",
                "--audio-quality", "0",
                "--output", str(output_path),
                "--no-playlist",
                "--max-filesize", "200M",
                url,
            ],
            capture_output=True,
            text=True,
            timeout=300,
        )
        if result.returncode != 0:
            job["status"] = JobStatus.FAILED
            job["error"] = f"Download failed: {result.stderr[:500]}"
            return None

        # Find the downloaded file
        for f in job_dir.iterdir():
            if f.name.startswith("input."):
                return f

        job["status"] = JobStatus.FAILED
        job["error"] = "Download produced no output file"
        return None

    except subprocess.TimeoutExpired:
        job["status"] = JobStatus.FAILED
        job["error"] = "Download timed out (5 minute limit)"
        return None
    except FileNotFoundError:
        job["status"] = JobStatus.FAILED
        job["error"] = "yt-dlp not found. Install it: pip install yt-dlp"
        return None


def _run_demucs(job_id: str, input_path: Path) -> bool:
    """Run Demucs separation on the input file. Returns True on success."""
    job = jobs[job_id]
    job["status"] = JobStatus.SEPARATING
    job["progress"] = 0.15
    job["message"] = "Running AI stem separation (this may take 30-60 seconds)..."

    job_dir = JOBS_DIR / job_id
    output_dir = job_dir / "separated"

    try:
        # Run demucs with htdemucs model (best quality)
        result = subprocess.run(
            [
                sys.executable, "-m", "demucs",
                "-n", "htdemucs",
                "--out", str(output_dir),
                str(input_path),
            ],
            capture_output=True,
            text=True,
            timeout=600,
        )

        if result.returncode != 0:
            job["status"] = JobStatus.FAILED
            job["error"] = f"Demucs failed: {result.stderr[:500]}"
            return False

        # Demucs outputs to: output_dir/htdemucs/<filename_without_ext>/<stem>.wav
        # Find the output directory
        demucs_out = output_dir / "htdemucs"
        if not demucs_out.exists():
            job["status"] = JobStatus.FAILED
            job["error"] = "Demucs produced no output directory"
            return False

        # Get the first (and only) subdirectory
        subdirs = [d for d in demucs_out.iterdir() if d.is_dir()]
        if not subdirs:
            job["status"] = JobStatus.FAILED
            job["error"] = "Demucs produced no stem files"
            return False

        stem_dir = subdirs[0]

        # Move stems to job root for clean access
        stems_found = {}
        for stem_name in STEM_NAMES:
            stem_file = stem_dir / f"{stem_name}.wav"
            if stem_file.exists():
                dest = job_dir / f"{stem_name}.wav"
                shutil.move(str(stem_file), str(dest))
                stems_found[stem_name] = str(dest)

        if not stems_found:
            job["status"] = JobStatus.FAILED
            job["error"] = "No stem WAV files found in Demucs output"
            return False

        job["stems"] = stems_found
        job["status"] = JobStatus.COMPLETE
        job["progress"] = 1.0
        job["message"] = f"Separation complete — {len(stems_found)} stems"

        # Cleanup intermediate files
        shutil.rmtree(str(output_dir), ignore_errors=True)

        return True

    except subprocess.TimeoutExpired:
        job["status"] = JobStatus.FAILED
        job["error"] = "Separation timed out (10 minute limit)"
        return False
    except FileNotFoundError:
        job["status"] = JobStatus.FAILED
        job["error"] = "Demucs not found. Install it: pip install demucs"
        return False


def _process_job(job_id: str, input_path: Path, is_url: bool, url: Optional[str] = None):
    """Background worker: download (if URL) then run Demucs."""
    try:
        if is_url and url:
            downloaded = _download_url(job_id, url)
            if downloaded is None:
                return
            input_path = downloaded

        # Simulate progress updates during separation
        job = jobs[job_id]

        def progress_ticker():
            while job["status"] == JobStatus.SEPARATING:
                if job["progress"] < 0.9:
                    job["progress"] = min(job["progress"] + 0.05, 0.9)
                time.sleep(3)

        ticker = threading.Thread(target=progress_ticker, daemon=True)
        ticker.start()

        _run_demucs(job_id, input_path)

    except Exception as e:
        jobs[job_id]["status"] = JobStatus.FAILED
        jobs[job_id]["error"] = str(e)


@app.get("/health")
async def health():
    """Health check endpoint for the DAW to verify service availability."""
    return {"status": "ok", "service": "stem-separator", "version": "1.0.0"}


@app.post("/separate")
async def separate_file(file: Optional[UploadFile] = File(None)):
    """
    Start a stem separation job from a file upload.
    Returns a job ID for polling.
    """
    if file is None:
        raise HTTPException(status_code=400, detail="No file provided. Upload a file or use POST /separate/url")

    job_id = _create_job()
    job_dir = JOBS_DIR / job_id

    # Save uploaded file
    ext = Path(file.filename).suffix if file.filename else ".wav"
    input_path = job_dir / f"input{ext}"
    with open(input_path, "wb") as f:
        content = await file.read()
        f.write(content)

    # Start background processing
    thread = threading.Thread(target=_process_job, args=(job_id, input_path, False), daemon=True)
    thread.start()

    return {"job_id": job_id, "status": JobStatus.PENDING}


@app.post("/separate/url")
async def separate_url(request: UrlRequest):
    """
    Start a stem separation job from a URL (YouTube, SoundCloud, Spotify).
    Returns a job ID for polling.
    """
    url = request.url.strip()
    if not url:
        raise HTTPException(status_code=400, detail="URL is required")

    job_id = _create_job()

    # Start background processing (download + separate)
    thread = threading.Thread(target=_process_job, args=(job_id, None, True, url), daemon=True)
    thread.start()

    return {"job_id": job_id, "status": JobStatus.PENDING}


@app.get("/status/{job_id}")
async def get_status(job_id: str):
    """Check the status of a separation job."""
    if job_id not in jobs:
        raise HTTPException(status_code=404, detail="Job not found")

    job = jobs[job_id]
    response = {
        "job_id": job_id,
        "status": job["status"],
        "progress": job["progress"],
        "message": job["message"],
    }

    if job["status"] == JobStatus.COMPLETE:
        response["stems"] = list(job["stems"].keys())
    elif job["status"] == JobStatus.FAILED:
        response["error"] = job["error"]

    return response


@app.get("/stems/{job_id}/{stem}")
async def get_stem(job_id: str, stem: str):
    """Download a separated stem WAV file."""
    if job_id not in jobs:
        raise HTTPException(status_code=404, detail="Job not found")

    job = jobs[job_id]
    if job["status"] != JobStatus.COMPLETE:
        raise HTTPException(status_code=400, detail=f"Job is not complete (status: {job['status']})")

    if stem not in STEM_NAMES:
        raise HTTPException(status_code=400, detail=f"Invalid stem name. Valid: {STEM_NAMES}")

    stem_path = JOBS_DIR / job_id / f"{stem}.wav"
    if not stem_path.exists():
        raise HTTPException(status_code=404, detail=f"Stem file not found: {stem}")

    return FileResponse(
        path=str(stem_path),
        media_type="audio/wav",
        filename=f"{stem}.wav",
    )


@app.delete("/jobs/{job_id}")
async def delete_job(job_id: str):
    """Clean up a completed job and its files."""
    if job_id not in jobs:
        raise HTTPException(status_code=404, detail="Job not found")

    job_dir = JOBS_DIR / job_id
    if job_dir.exists():
        shutil.rmtree(str(job_dir), ignore_errors=True)

    del jobs[job_id]
    return {"status": "deleted"}


if __name__ == "__main__":
    import uvicorn
    print("JamHub Stem Separator Service")
    print("Listening on http://localhost:8000")
    print("Endpoints:")
    print("  POST /separate       — upload a file for separation")
    print("  POST /separate/url   — separate from YouTube/SoundCloud/Spotify URL")
    print("  GET  /status/{id}    — check job progress")
    print("  GET  /stems/{id}/{s} — download a stem (vocals/drums/bass/other)")
    print()
    uvicorn.run(app, host="0.0.0.0", port=8000)
