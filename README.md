## Stuff

cargo depgraph --all-deps --dedup-transitive-deps | dot -Tpng > dependency_graph.png

cargo run -q | dot -Tpng > timestep_graph.png

```rust
let dot = bevy_mod_debugdump::schedule_graph_dot(
    &mut app,
    CoreSchedule::FixedUpdate,
    &schedule_graph::Settings::default(),
);
println!("{dot}");
```

cargo runwin

cargo tracewin

set "RUSTFLAGS=-C force-frame-pointers=y" & set "CARGO_BUILD_TARGET=x86_64-pc-windows-msvc" & set "CARGO_PROFILE_RELEASE_DEBUG=true" & cargo flamegraph

trunk serve
