//! A crate to allow `find` method on [futures_core::stream::Stream].
//! 
//! This crate provides a trait [StreamFind] which add method [StreamFind::find] to any
//! struct that implement trait [futures_core::stream::Stream] and [core::marker::Unpin].
//! It required [core::marker::Unpin] because, as currently is, [futures_core::stream::Stream] need
//! to be unpin to iterate over it.
//! 
//! The stream object is not consume so the stream can be reuse after the result is found.
//! 
//! The predicate function is different from [core::iter::Iterator::find]. The predicate function for
//! stream took owned value yield out of stream and pass ownership to predicate method.
//! If predicate function return [Option::None], it is the same as return false in [core::iter::Iterator::find].
//! If predicate function return [Option::Some], it is the same as return true in [core::iter::Iterator::find].
//! The value it pass back will be used as a found value so in most case, it should pass back the value it
//! pass in to predicate function.
//! 
//! The reason that it work this way is because, otherwise, it required self referential struct which is
//! required to be pinned. It will required unsafe code to make it work. To avoid using unsafe code completely,
//! The stream yielded value won't be hold by the struct. It passed in as owned value to predicate.
//! If predicate decided that it is not matched, the value is immediately dropped.
//! 
//! # Example
//! ```rust
//! # tokio_test::block_on(async {
//! use stream_find::StreamFind;
//! use futures::stream::{iter, StreamExt};
//! const START: usize = 0;
//! const END: usize = 100;
//! const TARGET: usize = 0;
//! let mut stream = iter(START..END);
//! let result = stream.find(async |item| {
//!     if item == TARGET {
//!         Some(item)
//!     } else {
//!         None
//!     }
//! }).await;
//! assert_eq!(result.unwrap(), TARGET, "Expect to found something.");
//! assert_eq!(stream.next().await.expect("to yield next value"), TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
//! # })
//! ```
use core::{pin::Pin, task::{Context, Poll}};

use futures_core::stream::Stream;

pub trait StreamFind: Stream + Unpin {
    fn find<'a, F, FR>(self: &'a mut Self, f: F) -> Find<'a, F, FR, Self> where Self: Sized, F: 'a + Unpin + FnMut(Self::Item) -> FR, FR: 'a + Future<Output = Option<Self::Item>>, Self::Item: Unpin {
        Find {
            poll_result: None,
            predicate: f,
            predicate_fut: None,
            stream: Pin::new(self)
        }
    }
}

pub struct Find<'a, F, FR, S> 
where 
    F: Unpin + FnMut(S::Item) -> FR, 
    FR: 'a + Future<Output = Option<S::Item>>, 
    S: Stream + Unpin, S::Item: Unpin 
{
    poll_result: Option<Poll<Option<S::Item>>>,
    predicate: F,
    predicate_fut: Option<Pin<Box<dyn Future<Output = Option<S::Item>> + 'a>>>,
    stream: Pin<&'a mut S>,
}

impl<'a, F, FR, S> Find<'a, F, FR, S> 
where 
    F: Unpin + FnMut(S::Item) -> FR, 
    FR: 'a + Future<Output = Option<S::Item>>, 
    S: Stream + Unpin, S::Item: Unpin 
{
    /// Check for value from stream by call a poll_next on it and update the state accordingly.
    /// 
    /// There's 3 possible states in this finding operation.
    /// 1. Wait for stream to return result.
    /// 2. Stream return result, need executor to poll predicate function.
    /// 3. Predicate function return result.
    /// 
    /// This function always reflect poll_result field to match current state.
    fn poll_stream(self: &mut Pin<&mut Self>, cx: &mut Context<'_>) {
        if let Poll::Ready(stream_poll_result) = self.stream.as_mut().poll_next(cx) {
            // Stream poll return some result
            if let Some(v) = stream_poll_result {
                // Stream return a value, pass value to predicate function now.
                self.predicate_fut = Some(Box::pin((self.predicate)(v)));
                // Mark that it need a polling because predicate checking also need polling to activate
                self.poll_result = None;
            } else {
                // Stream return None which mean it is depleted
                self.poll_result = Some(Poll::Ready(None));
            }
        } else {
            // Stream poll result is pending
            self.poll_result = Some(Poll::Pending);
        }
    }
}

impl<'a, F, FR, S> Future for Find<'a, F, FR, S> 
where 
    F: Unpin + FnMut(S::Item) -> FR, 
    FR: 'a + Future<Output = Option<S::Item>>, 
    S: Stream + Unpin, S::Item: Unpin 
{
    type Output = Option<S::Item>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Use while loop to take account when the stream is consume fast enough that may cause stack overflow
        // if we recursively poll to check predicate and stream.
        // This is due to nature of switching back and forth between two futures, polling a stream and polling a predicate.
        // When predicate return false, it need to poll stream for next result.
        // When stream return result, it need to poll predicate to check if it match.
        // If the stream consume slowly, there won't be any stack overflow problem.
        while self.as_ref().poll_result.is_none() {
            // Loop until it return any Poll result
            if let Some(pred_fn_poll) = &mut self.predicate_fut {
                if let Poll::Ready(matched) = pred_fn_poll.as_mut().poll(cx) {
                    if let Some(v) = matched {
                        // Found value. Immediately exit loop and yield a match.
                        return Poll::Ready(Some(v))
                    } else {
                        // Value mismatch, advance the stream.
                        self.poll_stream(cx);
                    }
                } else {
                    // Predicate function is still working.
                    return Poll::Pending // Immediately exit loop and tell executor that it is in pending state.
                }
            } else {
                // Need to wait for stream to return result
                self.poll_stream(cx);
            }
        }
        // If it reach here, that mean stream is polled. Take out poll_result so next call to poll will evaluate the future.
        return self.poll_result.take().expect("Unexpected state reached. Both stream and predicate doesn't mark for future polling but there's still no poll result.")
    }
}

impl<T> StreamFind for T where T: Stream + Unpin {}

#[cfg(test)]
mod tests {
    use super::StreamFind;
    use futures::stream::{iter, StreamExt};

    #[tokio::test]
    async fn test_basic_find() {
        const START: usize = 0;
        const END: usize = 100;
        const TARGET: usize = 0;
        let mut stream = iter(START..END);
        let result = stream.find(async |item| {
            if item == TARGET {
                Some(item)
            } else {
                None
            }
        }).await;
        assert_eq!(result.unwrap(), TARGET, "Expect to found something.");
        assert_eq!(stream.next().await.expect("to yield next value"), TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
    }
    #[tokio::test]
    async fn test_basic_not_found() {
        const START: usize = 0;
        const END: usize = 100;
        const TARGET: usize = 100;
        let mut stream = iter(START..END);
        let result = stream.find(async |item| {
            if item == TARGET {
                Some(item)
            } else {
                None
            }
        }).await;
        assert!(result.is_none(), "Expect to found nothing.");
        assert!(stream.next().await.is_none(), "Expect stream to be depleted.");
    }
    #[tokio::test]
    async fn test_find_twice_on_same_stream() {
        const START: usize = 0;
        const END: usize = 100;
        const FIRST_TARGET: usize = 10;
        const SECOND_TARGET: usize = 20;
        let mut stream = iter(START..END);
        let result = stream.find(async |item| {
            if item == FIRST_TARGET {
                Some(item)
            } else {
                None
            }
        }).await;
        assert_eq!(result.unwrap(), FIRST_TARGET, "Expect to found something.");
        assert_eq!(stream.next().await.expect("to yield next value"), FIRST_TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
        let result = stream.find(async |item| {
            if item == SECOND_TARGET {
                Some(item)
            } else {
                None
            }
        }).await;
        assert_eq!(result.unwrap(), SECOND_TARGET, "Expect to found something.");
        assert_eq!(stream.next().await.expect("to yield next value"), SECOND_TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
    }
    #[tokio::test]
    async fn test_find_twice_on_edge_stream() {
        const START: usize = 0;
        const END: usize = 100;
        const FIRST_TARGET: usize = 0;
        const SECOND_TARGET: usize = 99;
        let mut stream = iter(START..END);
        let result = stream.find(async |item| {
            if item == FIRST_TARGET {
                Some(item)
            } else {
                None
            }
        }).await;
        assert_eq!(result.unwrap(), FIRST_TARGET, "Expect to found something.");
        assert_eq!(stream.next().await.expect("to yield next value"), FIRST_TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
        let result = stream.find(async |item| {
            if item == SECOND_TARGET {
                Some(item)
            } else {
                None
            }
        }).await;
        assert_eq!(result.unwrap(), SECOND_TARGET, "Expect to found something.");
        assert!(stream.next().await.is_none(), "Expect stream to be depleted.");
    }
    #[tokio::test]
    async fn test_find_fail_on_second_find() {
        const START: usize = 0;
        const END: usize = 100;
        const FIRST_TARGET: usize = 11;
        const SECOND_TARGET: usize = 1; // Second target is in already yield out of stream in first find.
        let mut stream = iter(START..END);
        let result = stream.find(async |item| {
            if item == FIRST_TARGET {
                Some(item)
            } else {
                None
            }
        }).await;
        assert_eq!(result.unwrap(), FIRST_TARGET, "Expect to found something.");
        assert_eq!(stream.next().await.expect("to yield next value"), FIRST_TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
        let result = stream.find(async |item| {
            if item == SECOND_TARGET {
                Some(item)
            } else {
                None
            }
        }).await;
        println!("{:?}", result);
        assert!(result.is_none(), "Expect to found nothing.");
        assert!(stream.next().await.is_none(), "Expect stream to be depleted.");
    }
    #[tokio::test]
    async fn test_stack_overflow() {
        const START: usize = 0;
        // This value should be large enough but not too large to cause test to took forever to run.
        const END: usize = 99_000_000;
        let mut stream = iter(START..END);
        let result = stream.find(async |_item| None).await; // It should run until stream depleted
        assert!(result.is_none(), "Expect to found nothing.");
        assert!(stream.next().await.is_none(), "Expect stream to be depleted.");
    }
}