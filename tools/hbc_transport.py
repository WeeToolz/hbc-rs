"""Byte-stream reads shared by the HBC terminal clients."""


def recv_exact(connection, size):
    """Read the requested bytes or return the transport or truncation error."""
    if size < 0:
        raise ValueError("Reply size must not be negative")
    data = bytearray()
    while len(data) < size:
        chunk = connection.recv(size - len(data))
        if not chunk:
            raise OSError(f"Truncated reply: {len(data)}/{size} bytes")
        data.extend(chunk)
    return bytes(data)
