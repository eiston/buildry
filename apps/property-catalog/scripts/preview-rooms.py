#!/usr/bin/env python3
"""Room UI fixtures at :8083. All property writes stay in memory."""
import importlib.util
from pathlib import Path
from urllib.parse import urlparse
from http.server import ThreadingHTTPServer

spec = importlib.util.spec_from_file_location('task_preview', Path(__file__).with_name('preview-tasks.py'))
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
PROPERTIES = base.PROPERTIES
PROPERTIES[0]['spaces'] = [
    dict(id=11, name='Garden room', kind='Bedroom', bathroom_access='Ensuite', photos=['/assets/stay-regular-bedroom-1.jpg'], current_monthly_rent=1100),
    dict(id=12, name='Courtyard room', kind='Bedroom', bathroom_access='Separate', photos=[]),
    dict(id=13, name='Private suite', kind='Suite', photos=['/assets/missing-preview.jpg']),
    dict(id=14, name='A spacious room with a long name overlooking the garden and courtyard', kind='Bedroom', photos=[]),
]

class RoomPreview(base.TaskPreview):
    def do_GET(self):
        path = urlparse(self.path).path
        if path == '/api/properties': return self.respond(PROPERTIES)
        if path.startswith('/assets/stay-regular-bedroom-'):
            asset = Path(__file__).resolve().parent.parent / path.lstrip('/')
            data = asset.read_bytes()
            self.send_response(200)
            self.send_header('Content-Type', 'image/jpeg')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        return super().do_GET()

    def do_PUT(self):
        path = urlparse(self.path).path
        if path.startswith('/api/properties/'):
            data = base.base.json.loads(self.rfile.read(int(self.headers.get('Content-Length', 0))))
            scenario = urlparse(self.headers.get('Referer', '')).query
            if 'save-error' in scenario: return self.respond('Could not save room.', 503)
            existing = next((p for p in PROPERTIES if p['id'] == data['id']), None)
            if existing is None: PROPERTIES.append(data)
            else: existing.update(data)
            return self.respond(data)
        return super().do_PUT()

if __name__ == '__main__':
    print('Room preview: http://127.0.0.1:8083/properties/1', flush=True)
    ThreadingHTTPServer(('127.0.0.1', 8083), RoomPreview).serve_forever()
