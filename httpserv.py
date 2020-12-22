#!/usr/bin/python
import http.server
import socketserver

PORT = 8000

Handler = http.server.SimpleHTTPRequestHandler

Handler.extensions_map['.wasm'] = 'application/wasm'
httpd = socketserver.TCPServer(("", PORT), Handler)

print("http://localhost:" + str(PORT))
httpd.serve_forever()
