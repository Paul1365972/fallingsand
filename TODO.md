# Falling Sand

## TODO

- Falling Sand Sim
    - Oc
- Lighting
    - Terraria style lighting
    - Light Raytracing
- 

## Noita comparison 
1 tile is one color
Virtual Resolution: 424 x 242 (4.5 pixels per tile)
Region Chunk size: 512 x 512 (8 x 8 sim chunks)
Simulation Chunk size: 64 x 64
World size: 35840 x 73728 (70 x 144 Chunks)
12 Region chunks loaded for 3,145,728 "active" tiles (768 sim chunks)
| Game | Noita | Rust 4 | Rust 3 |
| -----|-------|--------|--------|
| Passes | 4 | 4 | 9 |
| Chunks per Pass | 196 | 196 | 85. |
| Unit size | 1 | 4x4 | 3x3 |
| Work units | 196 | 12 | 9. |

## Other Simulation stuff
- APIC
    - 4-16 Particles per cell simulated
- Position Based Dynamics
    - Possibly interesting
    - Somehow works together with pattern for physics
- Group particles and equalize their velocities (completely dampen local movement) also do sth with angular momentum (see 3.5 https://matthias-research.github.io/pages/publications/posBasedDyn.pdf)

## Optimizations
- Only update dirty chunks, generate dirty rects
- Use z-order hash for Chunk storage
- Z-order chunk index (and revamp grid coords)
- Slotmap (maybe dense) for chunk storage

## Game Update Order
    Client update order
    1. Recieve network packets
    2. Change 
    2. Wait for STEP packet (already received chunk packets at this point)

| Stage | Server | Host | Client | 
|-------|--------|------|--------| 
| / |   |  | _Always send movement packets_ | 
| 1. | **Receive new players** | **Receive new players** | **Receive network packets** | 
| 2. | **Insert chunks to region** | **Insert chunks to region** | **Apply (un?)load chunks** | 
| 3. | **Apply global events** | **Apply global events** | **Apply global events** | 
| 4. | **Freeze packets and send tick** | **Freeze packets/own input and send tick** | **Wait for tick packet (with movement info)** | 
| 5. | **Tick game** | **Tick game** | **Tick game** | 
| 6. |  | Preload and send global events, chunks etc. | (Send tick rsp finished) | 
| 7. |  | **Render Frame** | **Render Frame** | 
| 8. | (Wait for tick rsp if lockstep mode) | (Wait for tick rsp if lockstep mode) | | 

1. Tick tiles
2. Tick Entities
3. Apply entity dx/dy, change chunks


1. Player connects
2. Game notices before next tick and adds global event


## New design

### Run modes
1. Server: Orchestrator(multi worker) -> DisjointRegions
2. Client: Orchestrator(single worker) -> DisjointRegion
2. Replay: Orchestrator -> DisjointRegion
