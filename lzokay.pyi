"""Type stubs for lzokay - Pure Rust LZO compression library."""

# Exception hierarchy
class LzokayError(Exception):
    """Base exception for all lzokay errors."""

class LookbehindOverrunError(LzokayError):
    """Likely indicates bad compressed LZO input."""

class OutputOverrunError(LzokayError):
    """Output buffer was not large enough to store the compression/decompression result."""

class InputOverrunError(LzokayError):
    """Compressed input buffer is invalid or truncated."""

class LzokayUnknownError(LzokayError):
    """Unknown error."""

class InputNotConsumedError(LzokayError):
    """Decompression succeeded, but input buffer has remaining data."""

def compress(data: bytes) -> bytes:
    """
    Compress data using LZO compression.

    Args:
        data: The input bytes to compress

    Returns:
        The compressed data as bytes

    Raises:
        OutputOverrunError:
        LzokayUnknownError:
    """

def decompress(data: bytes, buffer_size: int) -> bytes:
    """
    Decompress LZO compressed data.

    Args:
        data: The compressed input bytes
        buffer_size: Expected size of the decompressed output

    Returns:
        The decompressed data as bytes

    Raises:
        LookbehindOverrunError:
        OutputOverrunError:
        InputOverrunError:
        InputNotConsumedError:
        LzokayUnknownError:
    """

def compress_worst_size(length: int) -> int:
    """Returns the worst-case size for LZO compression of data of given length."""
