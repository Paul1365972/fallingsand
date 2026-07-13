use crate::commands::PendingCommands;
use crate::dig::DigState;
use crate::inventory::SlotActions;
use crate::physics::StampResult;
use crate::player::{
    Air, Burning, Control, Health, Life, PLAYER_HALF_H, PLAYER_HALF_W, Player, PlayerActor,
    PlayerRaster,
};
use crate::{MAX_AIR_SECS, MAX_HP, SimWorld, SpawnPoint};
use bevy_ecs::prelude::*;
use fallingsand_core::Fixed;
use fallingsand_protocol::{InputState, LifeState};
use fallingsand_sim::physics::{Actor, Controller};

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)]
pub fn resolve_lifecycle(
    mut sim: ResMut<SimWorld>,
    spawn: Res<SpawnPoint>,
    mut bodies: ResMut<crate::bodies::PixelBodies>,
    mut impulses: ResMut<crate::PlayerImpulses>,
    mut slot_actions: ResMut<SlotActions>,
    mut commands: ResMut<PendingCommands>,
    mut query: Query<(
        Entity,
        &mut Player,
        &mut Life,
        &mut Health,
        &mut PlayerActor,
        &mut PlayerRaster,
        &mut Control,
        &mut DigState,
        &mut Air,
        &mut Burning,
    )>,
) {
    for (
        entity,
        mut player,
        mut life,
        mut health,
        mut body,
        mut raster,
        mut control,
        mut dig,
        mut air,
        mut burning,
    ) in &mut query
    {
        if life.0 == LifeState::Alive && health.hp <= 0.0 {
            health.hp = 0.0;
            life.0 = LifeState::Dead;
            clear_player_work(
                entity,
                &mut player,
                &mut control,
                &mut dig,
                &mut impulses,
                &mut slot_actions,
                &mut commands,
            );
            crate::physics::unstamp_and_wake(&mut sim.0, &mut bodies, &mut raster.0);
        }

        if life.0 != LifeState::Dead || !std::mem::take(&mut player.revive_requested) {
            continue;
        }
        let mut revived = Actor::new(
            Fixed::from_cell(spawn.0.x),
            Fixed::from_cell(spawn.0.y),
            PLAYER_HALF_W,
            PLAYER_HALF_H,
        );
        match crate::physics::spawn_stamp(&mut sim.0, &mut raster.0, &mut revived) {
            StampResult::Stamped => {}
            StampResult::Deferred => {
                player.revive_requested = true;
                continue;
            }
            StampResult::Blocked => continue,
        }
        body.0 = revived;
        control.0 = Controller::default();
        health.hp = MAX_HP;
        air.secs = MAX_AIR_SECS;
        burning.secs = 0.0;
        life.0 = LifeState::Alive;
        player.input = InputState::default();
        player.jump_pressed = false;
    }
}

fn clear_player_work(
    entity: Entity,
    player: &mut Player,
    control: &mut Control,
    dig: &mut DigState,
    impulses: &mut crate::PlayerImpulses,
    slot_actions: &mut SlotActions,
    commands: &mut PendingCommands,
) {
    player.input = InputState::default();
    player.jump_pressed = false;
    player.flying = false;
    player.revive_requested = false;
    control.0 = Controller::default();
    *dig = DigState::default();
    impulses.0.remove(&entity);
    slot_actions.0.retain(|action| action.entity != entity);
    commands.0.retain(|command| command.entity != entity);
}
