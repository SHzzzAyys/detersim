# Tutorial: deterministic stream API

`detersim-net` is for protocols that think in frames or socket-like streams.
It is not a real socket adapter and it does not use OS networking.

```rust
use detersim_net::{connect_pair, ConnectionId, StreamFault};

let mut stream = connect_pair(0, 1, ConnectionId(1));
stream.send(b"hello".to_vec(), &[]);
stream.send(b"again".to_vec(), &[StreamFault::Duplicate { seq: 1 }]);
let transcript = stream.into_transcript();
```

Use the transcript as protocol history or attach it to a debug artifact. Runtime
integration should still route scheduling, delays, drops, and crashes through
`World`, nemesis, and the entropy tape.
