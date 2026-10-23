"""Inert tool stand-in; every path and endpoint belongs to its story."""
import json
import os
from pathlib import Path
import socket
import sys

mode, *args = sys.argv[1:]
if mode == 'help':
    pass
elif mode == 'view':
    outside, read, write, work, endpoint, runner_namespace = args
    result = dict(read=Path(read).read_text(), stdin=sys.stdin.read(), env=dict(os.environ),
                  tmp=os.listdir('/tmp'), namespace=os.readlink('/proc/self/ns/pid') != runner_namespace)
    try:
        Path(outside).read_text()
        result['outside'] = True
    except OSError:
        result['outside'] = False
    for name, path in (('read_write', read), ('work_write', work + '/forbidden')):
        try:
            Path(path).write_text('forbidden')
            result[name] = True
        except OSError:
            result[name] = False
    Path(write, 'allowed').write_text('allowed')
    with socket.socket(socket.AF_UNIX) as connection:
        connection.connect(endpoint)
        connection.sendall(b'connected')
    print(json.dumps(result), flush=True)
elif mode == 'wait':
    endpoint, directory = args
    child = os.fork()
    if child == 0:
        os.setsid()
        with socket.socket(socket.AF_UNIX) as connection:
            connection.connect(endpoint)
            connection.sendall(b'child')
            connection.recv(1)
        os._exit(0)
    with socket.socket(socket.AF_UNIX) as connection:
        connection.connect(endpoint)
        connection.sendall(b'parent')
        connection.recv(1)
elif mode == 'large':
    sys.stdout.write('x' * 1_000_000)
    sys.stdout.flush()
elif mode == 'touch':
    Path(args[0]).write_text('ran')
else:
    raise ValueError(mode)
