//! Progress rule shared by the native fixture and its portable error/timeout checks.

use std::io;
use std::time::Instant;

pub(super) fn advance(
    input: &[u8],
    written: io::Result<usize>,
    closed: bool,
    deadline: Instant,
) -> io::Result<&[u8]> {
    let count = match written {
        Ok(0) if closed => {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "ConPTY input pipe closed"));
        },
        Ok(count) => count,
        Err(error)
            if !closed
                && matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) =>
        {
            0
        },
        Err(error) => return Err(error),
    };
    if Instant::now() >= deadline {
        return Err(io::Error::new(io::ErrorKind::TimedOut, "ConPTY input remained blocked"));
    }
    let pending = input
        .get(count..)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid ConPTY write count"))?;
    if closed && !pending.is_empty() {
        return Err(io::Error::new(io::ErrorKind::WriteZero, "ConPTY input pipe closed"));
    }
    Ok(pending)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(10)
    }

    #[test]
    fn partial_progress_keeps_exactly_the_unwritten_suffix() {
        assert_eq!(advance(b"enter", Ok(2), false, deadline()).unwrap(), b"ter");
        assert!(advance(b"enter", Ok(5), true, deadline()).unwrap().is_empty());
    }

    #[test]
    fn open_zero_and_transient_errors_keep_input_pending() {
        assert_eq!(advance(b"enter", Ok(0), false, deadline()).unwrap(), b"enter");
        for kind in [io::ErrorKind::Interrupted, io::ErrorKind::WouldBlock] {
            assert_eq!(advance(b"enter", Err(kind.into()), false, deadline()).unwrap(), b"enter");
            assert!(advance(b"enter", Err(kind.into()), true, deadline()).is_err());
        }
    }

    #[test]
    fn closed_zero_or_partial_input_cannot_be_retried() {
        for count in [0, 2] {
            assert_eq!(
                advance(b"enter", Ok(count), true, deadline()).unwrap_err().kind(),
                io::ErrorKind::WriteZero
            );
        }
    }

    #[test]
    fn real_errors_and_invalid_counts_remain_errors() {
        for kind in [io::ErrorKind::WriteZero, io::ErrorKind::BrokenPipe] {
            assert_eq!(
                advance(b"enter", Err(kind.into()), false, deadline()).unwrap_err().kind(),
                kind
            );
        }
        assert_eq!(
            advance(b"enter", Ok(6), false, deadline()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn a_deadline_bounds_zero_partial_and_transient_progress() {
        let expired = Instant::now() - Duration::from_secs(1);
        for written in [Ok(0), Ok(2), Err(io::ErrorKind::WouldBlock.into())] {
            assert_eq!(
                advance(b"enter", written, false, expired).unwrap_err().kind(),
                io::ErrorKind::TimedOut
            );
        }
    }
}
