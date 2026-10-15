"""private test CA and TLS loopback server, with no OS trust changes."""
import http.server
import pathlib
import ssl
import subprocess
import sys

root = pathlib.Path(sys.argv[1])

def openssl(*args):
    subprocess.run(["openssl", *args], cwd=root, check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)

openssl("req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
        "-subj", "/CN=OpenBindings private test CA", "-keyout", "ca.key", "-out", "ca.pem",
        "-addext", "basicConstraints=critical,CA:TRUE")
openssl("req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=127.0.0.1",
        "-keyout", "server.key", "-out", "server.csr")
(root / "extensions").write_text("subjectAltName=IP:127.0.0.1,IP:::1\nextendedKeyUsage=serverAuth\n")
openssl("x509", "-req", "-in", "server.csr", "-CA", "ca.pem", "-CAkey", "ca.key",
        "-CAcreateserial", "-days", "1", "-extfile", "extensions", "-out", "server.pem")

class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        with (root / "requests").open("a") as log:
            log.write(self.path + "\n")
        if self.path != "/done" and self.headers.get("x-case") == "downgrade-chain":
            self.send_response(302)
            self.send_header("Location", self.headers["x-location"])
            self.send_header("Content-Length", "0")
            self.end_headers()
        else:
            body = b'{"openbindings":"0.2.0","operations":{}}'
            self.send_response(200)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    def log_message(self, *args):
        pass

server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(root / "server.pem", root / "server.key")
server.socket = context.wrap_socket(server.socket, server_side=True)
print(f"https://127.0.0.1:{server.server_port}", flush=True)
server.serve_forever()
