use pyo3::prelude::*;
use pyo3::exceptions::PyException;

use crate::{compress, decompress, Error};

pyo3::create_exception!(lzokay, LzokayError, PyException, "Any kind of error.");

// Custom Python exception classes for each lzokay::Error variant
pyo3::create_exception!(lzokay, LookbehindOverrunError, LzokayError, "Likely indicates bad compressed LZO input.");
pyo3::create_exception!(lzokay, OutputOverrunError, LzokayError, "Output buffer was not large enough to store the compression/decompression result.");
pyo3::create_exception!(lzokay, InputOverrunError, LzokayError, "Compressed input buffer is invalid or truncated.");
pyo3::create_exception!(lzokay, LzokayUnknownError, LzokayError, "Unknown error.");
pyo3::create_exception!(lzokay, InputNotConsumedError, LzokayError, "Decompression succeeded, but input buffer has remaining data.");

// Helper function to convert lzokay::Error to appropriate Python exception
fn lzokay_error_to_pyerr(error: Error) -> PyErr {
    match error {
        Error::LookbehindOverrun => LookbehindOverrunError::new_err("lookbehind overrun"),
        Error::OutputOverrun => OutputOverrunError::new_err("output overrun"),
        Error::InputOverrun => InputOverrunError::new_err("input overrun"),
        Error::Error => LzokayUnknownError::new_err("unknown error"),
        Error::InputNotConsumed => InputNotConsumedError::new_err("input not consumed"),
    }
}

/// Decompress
#[pyfunction(name="decompress")]
fn py_decompress(data: &[u8], buffer_size: usize) -> PyResult<Vec<u8>> {
    let mut dst = vec![0u8; buffer_size];

    decompress::decompress(data, &mut dst).map_err(lzokay_error_to_pyerr)?;

    Ok(dst)
}

/// Compress data using LZO compression.
#[pyfunction(name="compress")]
fn py_compress(data: &[u8]) -> PyResult<Vec<u8>> {
    let ret = compress::compress(data).map_err(lzokay_error_to_pyerr)?;
    Ok(ret)
}

/// Returns the worst-case size for LZO compression of data of given length.
#[pyfunction(name="compress_worst_size")]
fn py_compress_worst_size(length: usize) -> PyResult<usize> {
    Ok(compress::compress_worst_size(length))
}

pub fn lzokay(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_decompress, m)?)?;
    m.add_function(wrap_pyfunction!(py_compress, m)?)?;
    m.add_function(wrap_pyfunction!(py_compress_worst_size, m)?)?;
    
    // Add exception classes to the module
    m.add("LzokayError", m.py().get_type::<LzokayError>())?;
    m.add("LookbehindOverrunError", m.py().get_type::<LookbehindOverrunError>())?;
    m.add("OutputOverrunError", m.py().get_type::<OutputOverrunError>())?;
    m.add("InputOverrunError", m.py().get_type::<InputOverrunError>())?;
    m.add("LzokayUnknownError", m.py().get_type::<LzokayUnknownError>())?;
    m.add("InputNotConsumedError", m.py().get_type::<InputNotConsumedError>())?;
    
    Ok(())
}
