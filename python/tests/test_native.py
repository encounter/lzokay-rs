import pytest

import lzokay


@pytest.mark.parametrize(
    "data",
    [
        b"Hello World",
        (
            b"Hello Worldello Worldello Worldello Worldello Worldello Worldello Worldello "
            b"Worldello Worldello Worldello Worldello Worldello Worldello Worldello World"
        ),
    ],
)
def test_compress_and_decompress(data):
    compressed = lzokay.compress(data)

    decompressed = lzokay.decompress(compressed, len(data))

    assert decompressed == data


def test_output_overrun_decompress():
    compressed = lzokay.compress(b"Hello World")

    with pytest.raises(lzokay.OutputOverrunError):
        lzokay.decompress(compressed, 1)


def test_input_overrun_decompress():
    with pytest.raises(lzokay.InputOverrunError):
        lzokay.decompress(b"", 1)


def test_input_not_consumed_decompress():
    compressed = lzokay.compress(b"Hello World")

    with pytest.raises(lzokay.InputNotConsumedError):
        lzokay.decompress(compressed + b"00000000000", len(compressed))
