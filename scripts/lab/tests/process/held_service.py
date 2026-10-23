"""Stand-in held-file service: moves opaque random test bytes, never prints them."""
import os
from pathlib import Path
import signal
import socket
import sys


def main():
    mode, *args = sys.argv[1:]
    if mode == 'serve':
        signal.signal(signal.SIGTERM, lambda _sig, _frame: sys.exit(0))
        print('ready', flush=True)
        while True:
            signal.pause()
    elif mode == 'seed':
        source, target, notification, wait = args
        os.rename(source, target)
        if notification != '-':
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
                connection.connect(notification)
                connection.sendall(b'moved\n')
        if wait == 'hold':
            while True:
                signal.pause()
    elif mode == 'return':
        source, target = args
        os.rename(source, target)
    elif mode == 'lose':
        Path(args[0]).unlink()
    else:
        raise ValueError('unknown stand-in mode')


if __name__ == '__main__':
    main()
