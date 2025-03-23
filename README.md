# stream-find
A crate that provide trat `StreamFind` which add method `find` to any `futures::stream::Stream` object.

## How to use
- `use stream_find::StreamFind;`
- `stream_obj.find(async |item| {// return true if match, otherwise return false})`
## Example
```rust
use stream_find::StreamFind;
use futures::stream::{iter, StreamExt};

const START: usize = 0;
const END: usize = 100;
const TARGET: usize = 0;
let mut stream = iter(START..END);
let result = stream.find(async |item| {
    *item == TARGET
}).await;
assert_eq!(result.unwrap(), TARGET, "Expect to found something.");
assert_eq!(stream.next().await.expect("to yield next value"), TARGET + 1, "Expect stream to be resumable and it immediately stop after it found first match.");
```

## Breaking change
### 0.2.0
Switch from manually implement Future trait on `Find` struct to relying on `futures::stream::StreamExt::next` method to yield a value and pass the yielded value as ref to predicate function. This greatly improve performance. It is showed in several tests that this approach is at least 7 times faster than the original.