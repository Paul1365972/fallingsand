use crate::controllers::Controller;
use crate::player::{Avatar, AvatarSnapshot, BodyIds, Health, PlayerLife, Players, SpawnSearch};
use crate::species::flesh::{self, DUCK_ROWS, STAND_ROWS};
use fallingsand_core::content::material;
use fallingsand_core::{CHUNK_SIZE, CellPos, CellRect, ChunkPos};
use fallingsand_protocol::PlayerId;
use fallingsand_sim::CellWorld;
use fallingsand_sim::body::{Bodies, Fracture, Policy};

const SEARCH_ATTEMPTS_PER_TICK: usize = CHUNK_SIZE;

pub fn begin_revives(players: &mut Players, spawn: CellPos, tick: u64) {
    for (_, player) in players.iter_mut() {
        if !std::mem::take(&mut player.inbox.revive_requested) {
            continue;
        }
        player.begin_revive(spawn, tick);
    }
}

pub fn resolve_lethal(
    sim: &mut CellWorld,
    bodies: &mut Bodies,
    players: &mut Players,
    tick: u64,
) -> Vec<String> {
    let dying: Vec<PlayerId> = players
        .iter()
        .filter_map(|(&id, player)| {
            player
                .avatar()
                .is_some_and(|avatar| avatar.health.hp <= 0.0)
                .then_some(id)
        })
        .collect();

    let mut died = Vec::new();
    for id in dying {
        let Some(player) = players.get_mut(id) else {
            continue;
        };
        let anchor = player.view_anchor(bodies);
        let PlayerLife::Alive(avatar) = &mut player.life else {
            continue;
        };
        let body_id = avatar.body_id;
        bodies.die(body_id);
        bodies.recast(sim, body_id, material::CORPSE);
        player.die(anchor, tick);
        died.push(player.name.clone());
    }
    died
}

pub fn resolve_fractures(
    sim: &mut CellWorld,
    bodies: &mut Bodies,
    players: &mut Players,
    fractures: &[Fracture],
    tick: u64,
) -> Vec<String> {
    let mut died = Vec::new();
    for fracture in fractures {
        let player_id = players.iter().find_map(|(&id, player)| {
            player
                .avatar()
                .is_some_and(|avatar| avatar.body_id == fracture.source)
                .then_some(id)
        });
        let Some(player_id) = player_id else {
            continue;
        };
        for &part in &fracture.parts {
            bodies.die(part);
            bodies.recast(sim, part, material::CORPSE);
        }
        let player = players
            .get_mut(player_id)
            .expect("fractured player remains present");
        player.die(fracture.anchor, tick);
        died.push(player.name.clone());
    }
    died
}

pub fn advance_materializations(
    sim: &mut CellWorld,
    bodies: &mut Bodies,
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
            bodies,
            body_ids,
            &materialization.template,
            &mut materialization.search,
        );
        match result {
            SearchResult::Waiting => {}
            SearchResult::Found(avatar) => player.finish_materialization(*avatar, tick),
            SearchResult::Exhausted => {
                let anchor = player.view_anchor(bodies);
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
    bodies: &mut Bodies,
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
        let rect = flesh::rect(candidate, STAND_ROWS);
        if !window.contains(rect.min) || !window.contains(rect.max) {
            search.center_window(candidate);
            return SearchResult::Waiting;
        }
        if let Some(avatar) = try_materialize(sim, bodies, body_ids, template, candidate) {
            return SearchResult::Found(Box::new(avatar));
        }
        if !search.advance() {
            return SearchResult::Exhausted;
        }
    }
    SearchResult::Waiting
}

fn window_loaded(sim: &CellWorld, window: CellRect) -> bool {
    let min = window.min.chunk();
    let max = window.max.chunk();
    for y in min.y..=max.y {
        for x in min.x..=max.x {
            if sim.chunk(ChunkPos::new(x, y)).is_none() {
                return false;
            }
        }
    }
    true
}

pub fn rect_loaded(sim: &CellWorld, rect: CellRect) -> bool {
    rect.cells().all(|pos| sim.get_cell(pos).is_some())
}

pub fn try_materialize(
    sim: &mut CellWorld,
    bodies: &mut Bodies,
    body_ids: &mut BodyIds,
    template: &AvatarSnapshot,
    candidate: CellPos,
) -> Option<Avatar> {
    if !rect_loaded(sim, flesh::rect(candidate, STAND_ROWS)) {
        return None;
    }
    let feet = flesh::feet(candidate, STAND_ROWS);
    let body_id = body_ids.allocate();
    for rows in (DUCK_ROWS..=STAND_ROWS).rev() {
        let cells = flesh::cells(flesh::anchor(candidate.x, feet, rows), rows, false);
        if !bodies.spawn(sim, body_id, &cells, Policy::PLAYER) {
            continue;
        }
        bodies.drive(body_id, template.vx, template.vy);
        return Some(Avatar {
            body_id,
            controller: Controller::new(rows),
            health: Health {
                hp: template.hp.clamp(0.0, crate::MAX_HEALTH),
                regen_delay_ticks: template.regen_delay_ticks,
            },
            air: template.air.clamp(0.0, crate::MAX_AIR_SECONDS),
            burning_secs: template.burning.max(0.0),
            flying: template.flying,
            dig: Default::default(),
        });
    }
    None
}
