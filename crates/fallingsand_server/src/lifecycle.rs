use crate::physics::{try_materialize, unstamp};
use crate::player::{AvatarSnapshot, BodyIds, PlayerLife, Players, SearchWindow, SpawnSearch};
use fallingsand_core::{CHUNK_SIZE, CellPos};
use fallingsand_protocol::PlayerId;
use fallingsand_sim::CellWorld;

const SEARCH_ATTEMPTS_PER_TICK: usize = CHUNK_SIZE;

pub fn begin_revives(players: &mut Players, spawn: CellPos, tick: u64) {
    for (_, player) in players.iter_mut() {
        if !std::mem::take(&mut player.control.revive_requested) {
            continue;
        }
        player.begin_revive(spawn, tick);
    }
}

pub fn resolve_lethal(sim: &mut CellWorld, players: &mut Players, tick: u64) {
    let dying: Vec<PlayerId> = players
        .iter()
        .filter_map(|(&id, player)| {
            player
                .avatar()
                .is_some_and(|avatar| avatar.health.hp <= 0.0)
                .then_some(id)
        })
        .collect();

    for id in dying {
        let Some(player) = players.get_mut(id) else {
            continue;
        };
        let anchor = player.view_anchor();
        let PlayerLife::Alive(avatar) = &mut player.life else {
            continue;
        };
        let body_id = avatar.body_id;
        unstamp(sim, &mut avatar.stamp, body_id);
        player.die(anchor, tick);
        players.unregister_body(body_id);
    }
}

pub fn advance_materializations(
    sim: &mut CellWorld,
    players: &mut Players,
    body_ids: &mut BodyIds,
    tick: u64,
) -> Vec<(PlayerId, String)> {
    let mut failures = Vec::new();
    let ids: Vec<PlayerId> = players.iter().map(|(&id, _)| id).collect();
    for id in ids {
        let Some(player) = players.get_mut(id) else {
            continue;
        };
        let Some(materialization) = player.life.materialization_mut() else {
            continue;
        };
        let result = advance_search(
            sim,
            body_ids,
            &materialization.template,
            &mut materialization.search,
        );
        match result {
            SearchResult::Waiting => {}
            SearchResult::Found(avatar) => {
                let body_id = avatar.body_id;
                player.finish_materialization(*avatar, tick);
                players.register_body(body_id, id);
            }
            SearchResult::Exhausted => {
                let anchor = player.view_anchor();
                player.die(anchor, tick);
                failures.push((id, "no representable spawn position remains".into()));
            }
        }
    }
    failures
}

enum SearchResult {
    Waiting,
    Found(Box<crate::player::Avatar>),
    Exhausted,
}

fn advance_search(
    sim: &mut CellWorld,
    body_ids: &mut BodyIds,
    template: &AvatarSnapshot,
    search: &mut SpawnSearch,
) -> SearchResult {
    let window = search.window();
    if !window_loaded(sim, window) {
        return SearchResult::Waiting;
    }
    for _ in 0..SEARCH_ATTEMPTS_PER_TICK {
        let Some(candidate) = search.candidate() else {
            return SearchResult::Exhausted;
        };
        if !footprint_inside_window(candidate, window) {
            search.center_window(candidate);
            return SearchResult::Waiting;
        }
        if let Some(avatar) = try_materialize(sim, body_ids, template, candidate) {
            return SearchResult::Found(Box::new(avatar));
        }
        if !search.advance() {
            return SearchResult::Exhausted;
        }
    }
    SearchResult::Waiting
}

fn footprint_inside_window(candidate: CellPos, window: SearchWindow) -> bool {
    let fp = fallingsand_sim::player::player_shape(fallingsand_sim::player::STAND_ROWS).footprint(
        fallingsand_core::Subcell::from_cell(candidate.x),
        fallingsand_core::Subcell::from_cell(candidate.y),
    );
    window.contains(CellPos::new(fp.x0, fp.y0)) && window.contains(CellPos::new(fp.x1, fp.y1))
}

fn window_loaded(sim: &CellWorld, window: SearchWindow) -> bool {
    let min = window.min.chunk();
    let max = window.max.chunk();
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            if sim.chunk(fallingsand_core::ChunkPos::new(x, y)).is_none() {
                return false;
            }
        }
    }
    true
}
