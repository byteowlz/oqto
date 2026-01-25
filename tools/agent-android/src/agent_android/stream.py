"""Screencast streaming via py-scrcpy-client."""

import asyncio
import base64
import json
from typing import Optional, Callable
import io

# Note: py-scrcpy-client is optional dependency
try:
    import scrcpy

    SCRCPY_AVAILABLE = True
except ImportError:
    SCRCPY_AVAILABLE = False

try:
    from PIL import Image

    PIL_AVAILABLE = True
except ImportError:
    PIL_AVAILABLE = False

from .device import get_device


class ScreencastServer:
    """WebSocket server for streaming Android screen."""

    def __init__(self, port: int = 8765, quality: int = 80):
        self.port = port
        self.quality = quality
        self.client: Optional["scrcpy.Client"] = None
        self._running = False
        self._connections: set = set()

    async def start(self):
        """Start the screencast server."""
        if not SCRCPY_AVAILABLE:
            raise RuntimeError("py-scrcpy-client not installed. Run: pip install py-scrcpy-client")

        device = get_device()

        # Create scrcpy client
        self.client = scrcpy.Client(device=device.serial)
        self.client.add_listener(scrcpy.EVENT_FRAME, self._on_frame)

        # Start scrcpy in background thread
        self.client.start(threaded=True)
        self._running = True

        # Start WebSocket server
        import websockets

        async def handler(websocket, path):
            self._connections.add(websocket)
            try:
                # Keep connection open
                async for message in websocket:
                    # Handle incoming messages (e.g., input events)
                    await self._handle_input(message)
            finally:
                self._connections.remove(websocket)

        server = await websockets.serve(handler, "localhost", self.port)
        print(f"Screencast server running on ws://localhost:{self.port}")

        try:
            await asyncio.Future()  # Run forever
        finally:
            server.close()
            self.stop()

    def stop(self):
        """Stop the screencast server."""
        self._running = False
        if self.client:
            self.client.stop()

    def _on_frame(self, frame):
        """Handle new frame from scrcpy."""
        if not PIL_AVAILABLE:
            return

        # Convert frame to JPEG
        img = Image.fromarray(frame)
        buffer = io.BytesIO()
        img.save(buffer, format="JPEG", quality=self.quality)
        jpeg_data = buffer.getvalue()

        # Send to all connected clients
        message = json.dumps(
            {
                "type": "frame",
                "data": base64.b64encode(jpeg_data).decode("ascii"),
                "width": img.width,
                "height": img.height,
            }
        )

        # Schedule sending to all connections
        for ws in self._connections:
            asyncio.create_task(ws.send(message))

    async def _handle_input(self, message: str):
        """Handle input events from frontend."""
        try:
            data = json.loads(message)
            event_type = data.get("type")

            if event_type == "tap":
                x, y = data["x"], data["y"]
                if self.client:
                    self.client.control.tap(x, y)

            elif event_type == "swipe":
                x1, y1 = data["x1"], data["y1"]
                x2, y2 = data["x2"], data["y2"]
                duration = data.get("duration", 300)
                if self.client:
                    self.client.control.swipe(x1, y1, x2, y2, duration)

            elif event_type == "key":
                keycode = data["keycode"]
                if self.client:
                    self.client.control.keycode(keycode)

            elif event_type == "text":
                text = data["text"]
                if self.client:
                    self.client.control.text(text)

        except (json.JSONDecodeError, KeyError):
            pass


def start_screencast(port: int = 8765):
    """Start screencast streaming.

    Args:
        port: WebSocket port
    """
    server = ScreencastServer(port=port)
    asyncio.run(server.start())


def get_frame_jpeg(quality: int = 80) -> bytes:
    """Get a single frame as JPEG.

    Args:
        quality: JPEG quality (0-100)

    Returns:
        JPEG image data.
    """
    if not SCRCPY_AVAILABLE:
        # Fallback to screenshot
        device = get_device()
        img = device.screenshot()

        if PIL_AVAILABLE:
            buffer = io.BytesIO()
            img.save(buffer, format="JPEG", quality=quality)
            return buffer.getvalue()
        else:
            raise RuntimeError("PIL not available for image conversion")

    device = get_device()
    client = scrcpy.Client(device=device.serial, max_fps=1)

    frame_data = {"frame": None}

    def on_frame(frame):
        frame_data["frame"] = frame
        raise StopIteration

    client.add_listener(scrcpy.EVENT_FRAME, on_frame)

    try:
        client.start(threaded=False)
    except StopIteration:
        pass
    finally:
        client.stop()

    if frame_data["frame"] is not None and PIL_AVAILABLE:
        img = Image.fromarray(frame_data["frame"])
        buffer = io.BytesIO()
        img.save(buffer, format="JPEG", quality=quality)
        return buffer.getvalue()

    raise RuntimeError("Failed to capture frame")
