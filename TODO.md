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