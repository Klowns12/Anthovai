# The OCR sidecar: turns scanned pages into Markdown for the worker.
#
# The model itself is not in this image. It runs in Ollama — on the host's GPU
# in development, on a GPU node in production — and this container reaches it
# at OCR_OLLAMA_URL. That keeps the image small and the model's lifecycle
# (pulling, upgrading, sharing between services) where Ollama already handles it.
#
# Build from this directory:
#   docker build -t anthovai-ocr-sidecar:dev .

FROM python:3.12-slim

# Nothing here needs root.
RUN useradd --system --uid 10002 --create-home --home-dir /app ocr
WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY app.py extract.py ./

USER ocr
EXPOSE 9090

# Inside the compose network the worker reaches this by service name, so the
# bind is the container's interface, not loopback. The network itself is what
# keeps it off the internet.
ENV OCR_OLLAMA_URL=http://host.docker.internal:11434 \
    OCR_MODEL=scb10x/typhoon-ocr1.5-3b

# Healthy means the model is pulled and Ollama answers — `/health` returns 503
# otherwise — so the worker's retries stop the moment OCR is really ready.
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:9090/health', timeout=4)" || exit 1

CMD ["uvicorn", "app:app", "--host", "0.0.0.0", "--port", "9090", "--log-level", "info"]
