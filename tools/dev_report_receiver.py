"""Bounded test receiver for the production development report protocol."""
import socket
import struct
import threading


class ReportReceiver:
    def __init__(self, host, port, mode):
        self.mode = mode
        self.progress = []
        self.final = None
        self.error = None
        self.bytes_received = 0
        self.connection = None
        self.listener = socket.socket()
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind((host, port))
        self.listener.listen(1)
        self.listener.settimeout(.25)
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self._run, daemon=True)

    def start(self):
        self.thread.start()

    def _read(self, count):
        data = bytearray()
        while len(data) < count:
            part = self.connection.recv(count - len(data))
            if not part:
                raise EOFError('Report ended before the declared payload completed')
            data.extend(part)
            self.bytes_received += len(part)
        return bytes(data)

    def _run(self):
        try:
            while not self.stop.is_set():
                try:
                    self.connection, _ = self.listener.accept()
                    break
                except socket.timeout:
                    continue
            if self.connection is None:
                return
            self.connection.settimeout(35)
            while True:
                magic = self._read(4)
                if magic == b'WIP1':
                    header = magic + self._read(20)
                    version, flags, sequence, stage, run_len, build_len, text_len = struct.unpack('>HHIIHHI', header[4:])
                    if version != 1 or run_len > 64 or build_len != 64 or text_len > 32768:
                        raise ValueError('Invalid progress bounds or version')
                    payload = self._read(run_len + build_len + text_len)
                    self.progress.append((sequence, stage, flags, payload[:run_len], payload[run_len:run_len + build_len], payload[run_len + build_len:]))
                elif magic == b'WIR2':
                    header = magic + self._read(152)
                    version, flags, length = struct.unpack('>HHI', header[4:12])
                    stats_len = int.from_bytes(header[24:28], 'big')
                    if version != 2 or length > 32768 or stats_len != 36 or any(header[12:24]):
                        raise ValueError('Invalid final report layout')
                    self.final = (flags, header[28:92].rstrip(b'\0'), header[92:156], self._read(length), self._read(stats_len))
                    return
                else:
                    raise ValueError('Unknown report magic')
        except (OSError, EOFError, ValueError) as error:
            self.error = str(error)

    def finish(self):
        self.thread.join(timeout=5)
        if self.thread.is_alive():
            raise TimeoutError('Report receiver did not complete')

    def validate_failure(self):
        if self.final is not None or self.bytes_received < 24 or self.error is None:
            raise RuntimeError('Partial report failure was not observed by the host')
        return {'bytes_received': self.bytes_received, 'error': self.error}

    def validate(self, build, run, mode):
        if self.error:
            raise RuntimeError(self.error)
        if self.final is None:
            raise RuntimeError('Final production report is absent')
        flags, received_run, received_build, text, stats = self.final
        if received_run != run.encode() or received_build != build.encode():
            raise RuntimeError('Final report identity mismatch')
        for index, record in enumerate(self.progress, 1):
            sequence, stage, _, progress_run, progress_build, _ = record
            if sequence != index or stage != 0 or progress_run != run.encode() or progress_build != build.encode():
                raise RuntimeError('Progress identity, sequence, or stage mismatch')
        should_pass = mode == 'return-zero'
        expected = b'PASS summary passes=1 fails=0\n' if should_pass else b'FAIL summary passes=0 fails=1\n'
        if expected not in text or (not should_pass and b'PASS summary' in text):
            raise RuntimeError('Final report did not preserve application failure or log loss')
        if mode == 'return-error' and b'main exit status=-7' not in text:
            raise RuntimeError('Original application error is absent')
        if mode == 'log-overflow' and flags != 1:
            raise RuntimeError('Lost log bytes did not set the truncation flag')
        if stats != bytes(36):
            raise RuntimeError('Unexpected development statistics')
        return {'progress_count': len(self.progress), 'flags': flags, 'text': text.decode('utf-8', errors='replace')}

    def close(self):
        self.stop.set()
        if self.connection is not None:
            try:
                self.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass  # A closed peer needs no shutdown; close still runs.
            self.connection.close()
        self.listener.close()
        self.thread.join(timeout=2)
        if self.thread.is_alive():
            raise RuntimeError('Report receiver cleanup did not complete')
