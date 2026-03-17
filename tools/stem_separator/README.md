# JamHub Stem Separator Service

AI-powered stem separation using Meta's Demucs model. Separates mixed audio into vocals, drums, bass, and other stems.

## Setup

```bash
cd tools/stem_separator
pip install -r requirements.txt
python server.py
```

The service runs on http://localhost:8000.

## Endpoints

- **POST /separate** — Submit a separation job (file upload or URL)
- **GET /status/{job_id}** — Poll job progress
- **GET /stems/{job_id}/{stem}** — Download a separated stem WAV file

## Usage

### Separate a local file
```bash
curl -X POST http://localhost:8000/separate \
  -F "file=@song.mp3"
```

### Separate from URL (YouTube, SoundCloud, Spotify)
```bash
curl -X POST http://localhost:8000/separate \
  -H "Content-Type: application/json" \
  -d '{"url": "https://www.youtube.com/watch?v=..."}'
```

### Check job status
```bash
curl http://localhost:8000/status/{job_id}
```

### Download a stem
```bash
curl -o vocals.wav http://localhost:8000/stems/{job_id}/vocals
```

## Requirements

- Python 3.9+
- ~4GB disk space for the Demucs model (downloaded on first run)
- GPU recommended but not required (CPU works, just slower)
