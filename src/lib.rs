//! A crate to allow `find` method on [futures_core::stream::Stream].
//! 
//! This crate provides a trait [StreamFind] which add method [StreamFind::find] to any
//! struct that implement trait [futures_core::stream::Stream] and [core::marker::Unpin].
//! It required [core::marker::Unpin] because, as currently is, [futures_core::stream::Stream] need
//! to be unpin to iterate over it.
//! 
//! The stream object is not consume so the stream can be use after the result is found.
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
//!     *item == TARGET
//! }).await;
//! assert_eq!(result.unwrap(), TARGET, "Expect to found something.");
//! assert_eq!(stream.next().await.expect("to yield next value"), TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
//! # })
//! ```
use futures::stream::{Stream, StreamExt};

/// Add find method to any stream that can be unpinned.
/// 
/// # How to use
/// 1. `use stream_find::StreamFind`.
/// 2. Any struct that implement `futures::stream::Stream` and `core::marker::Unpin` will now have method `find`
/// similar to that of Iterator trait.
pub trait StreamFind: Stream + Unpin {
    /// Find the first item from stream that async closure return `Some` value.
    /// 
    /// # Example
    /// ```rust
    /// # tokio_test::block_on(async {
    /// use stream_find::StreamFind;
    /// use futures::stream::{iter, StreamExt};
    /// const START: usize = 0;
    /// const END: usize = 100;
    /// const TARGET: usize = 0;
    /// let mut stream = iter(START..END);
    /// let result = stream.find(async |item| {
    ///     *item == TARGET
    /// }).await;
    /// assert_eq!(result.unwrap(), TARGET, "Expect to found something.");
    /// assert_eq!(stream.next().await.expect("to yield next value"), TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
    /// # })
    /// ```
    fn find<P>(&mut self, mut predicate: P) -> impl Future<Output = Option<Self::Item>> where P: AsyncFnMut(&Self::Item) -> bool {
        async move {
            while let Some(item) = self.next().await {
                if (predicate)(&item).await {
                    return Some(item)
                }
            }
            None
        }
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
            *item == TARGET
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
            *item == TARGET
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
            *item == FIRST_TARGET
        }).await;
        assert_eq!(result.unwrap(), FIRST_TARGET, "Expect to found something.");
        assert_eq!(stream.next().await.expect("to yield next value"), FIRST_TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
        let result = stream.find(async |item| {
            *item == SECOND_TARGET
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
            *item == FIRST_TARGET
        }).await;
        assert_eq!(result.unwrap(), FIRST_TARGET, "Expect to found something.");
        assert_eq!(stream.next().await.expect("to yield next value"), FIRST_TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
        let result = stream.find(async |item| {
            *item == SECOND_TARGET
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
            *item == FIRST_TARGET
        }).await;
        assert_eq!(result.unwrap(), FIRST_TARGET, "Expect to found something.");
        assert_eq!(stream.next().await.expect("to yield next value"), FIRST_TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
        let result = stream.find(async |item| {
            *item == SECOND_TARGET
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
        let result = stream.find(async |_item| false).await; // It should run until stream depleted
        assert!(result.is_none(), "Expect to found nothing.");
        assert!(stream.next().await.is_none(), "Expect stream to be depleted.");
    }
}