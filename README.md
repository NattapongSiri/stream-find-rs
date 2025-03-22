# stream-find
A crate that provide trat `StreamFind` which add method `find` to any `futures::stream::Stream` object.

## How to use
- `use stream_find::StreamFind;`
- `stream_obj.find(async |item| {// Return Some(item) if found, otherwise return None})`
## Example
```rust
use stream_find::StreamFind;
use futures::stream::{iter, StreamExt};

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
```