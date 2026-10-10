"""Verify explicit acquisition boundaries with HTTP/FIFO positive controls.

Requires a POSIX FIFO host, loopback listeners, and a previously built tools/no-io
consumer. Dependency retrieval features are deliberately unified in that consumer.
This is an active regression witness, not a proof of all possible absence of I/O.
"""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import argparse
import errno
import hashlib
import json
import os
import subprocess
import threading

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if not hasattr(os, 'mkfifo'):
        raise SystemExit('This complete HTTP/FIFO control requires a POSIX FIFO host.')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve()
    hits, records, reader_errors = [], [], []

    class Trap(BaseHTTPRequestHandler):
        def do_GET(self):
            hits.append({'kind': 'http', 'path': self.path})
            self.send_response(200)
            self.send_header('Content-Length', '5')
            self.end_headers()
            self.wfile.write(b'false')

        def log_message(self, *args):
            pass

    server = ThreadingHTTPServer(('127.0.0.1', 0), Trap)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    pipe = output / 'schema.fifo'
    os.mkfifo(pipe)
    stopped = threading.Event()

    def writer():
        while not stopped.is_set():
            try:
                fd = os.open(pipe, os.O_WRONLY | os.O_NONBLOCK)
                hits.append({'kind': 'file', 'path': 'schema.fifo'})
                try:
                    os.write(fd, b'false')
                finally:
                    os.close(fd)
                stopped.wait(.02)
            except OSError as error:
                if error.errno != errno.ENXIO:
                    reader_errors.append(str(error))
                    return
                stopped.wait(.001)

    reader = threading.Thread(target=writer, daemon=True)
    reader.start()
    failure = None
    try:
        for kind, uri in [('http', f'http://127.0.0.1:{server.server_port}/schema'), ('file', pipe.as_uri())]:
            for mode in ['positive', 'sdk']:
                before = len(hits)
                command = [str(binary), mode, uri]
                result = subprocess.run(command, capture_output=True, text=True, timeout=15)
                (output / f'{kind}-{mode}.stdout').write_text(result.stdout)
                (output / f'{kind}-{mode}.stderr').write_text(result.stderr)
                observed = hits[before:]
                records.append({'kind': kind, 'mode': mode, 'command': command,
                                'exit': result.returncode, 'hits': observed})
                if result.returncode or bool(observed) != (mode == 'positive') or reader_errors:
                    raise RuntimeError(f'{kind}/{mode} control failed; inspect receipt')
    except Exception as error:
        failure = str(error)
    finally:
        stopped.set()
        reader.join(timeout=1)
        server.shutdown()
        server.server_close()
        thread.join(timeout=1)
        pipe.unlink()
    report = {
        'source': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'sourceStatus': subprocess.check_output(['git', 'status', '--short'], cwd=ROOT, text=True),
        'binarySha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
        'status': 'failed' if failure else 'passed', 'failure': failure, 'records': records,
        'readerErrors': reader_errors,
        'features': ['resolve-http', 'resolve-file', 'tls-ring'],
        'scope': 'SDK core assessment, external reference/dialect, kind/content, supplied resource; explicit raw dependency positive controls',
        'buildIdentity': 'Caller must bind binary to final source through its recorded build command/metadata.',
    }
    (output / 'REPORT.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'report': str(output / 'REPORT.json'), 'status': report['status']}))
    if failure:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
