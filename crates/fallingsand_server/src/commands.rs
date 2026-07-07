use crate::session::{SessionState, Sessions};
use crate::systems::Mode;
use bevy_ecs::prelude::*;
use fallingsand_protocol::{GameMode, ServerMessage, encode_message};

pub struct PendingCommand {
    pub entity: Entity,
    pub text: String,
}

#[derive(Resource, Default)]
pub struct PendingCommands(pub Vec<PendingCommand>);

pub type CommandRun = fn(&mut World, Entity, &[&str]) -> Result<Option<String>, String>;

pub struct CommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub usage: &'static str,
    pub run: CommandRun,
}

pub const COMMANDS: &[CommandSpec] = &[GAMEMODE, TIME];

const GAMEMODE: CommandSpec = CommandSpec {
    name: "gamemode",
    aliases: &["gm"],
    usage: "/gamemode <survival|creative|s|c>",
    run: |world, entity, args| {
        let mode = match args {
            [arg] => GameMode::parse(arg),
            _ => None,
        }
        .ok_or_else(|| format!("usage: {}", GAMEMODE.usage))?;
        let mut current = world
            .get_mut::<Mode>(entity)
            .ok_or_else(|| "player not in world".to_string())?;
        if current.0 == mode {
            return Ok(Some(format!("already in {} mode", mode.label())));
        }
        current.0 = mode;
        Ok(Some(format!("game mode set to {}", mode.label())))
    },
};

const TIME: CommandSpec = CommandSpec {
    name: "time",
    aliases: &[],
    usage: "/time <day|night|noon|midnight|+days>",
    run: |world, _entity, args| {
        let [arg] = args else {
            return Err(format!("usage: {}", TIME.usage));
        };
        let mut clock = world.resource_mut::<crate::WorldClock>();
        match *arg {
            "day" | "noon" => clock.t = 0.5,
            "night" | "midnight" => {
                clock.t = 0.0;
                clock.day += 1;
            }
            arg => {
                let days: u32 = arg
                    .strip_prefix('+')
                    .and_then(|days| days.parse().ok())
                    .ok_or_else(|| format!("usage: {}", TIME.usage))?;
                clock.day = clock.day.saturating_add(days);
            }
        }
        let (t, day) = (clock.t, clock.day);
        Ok(Some(format!("time set to {t:.2} of day {day}")))
    },
};

pub fn parse(text: &str) -> Option<(&str, Vec<&str>)> {
    let text = text.strip_prefix('/')?;
    let mut parts = text.split_whitespace();
    let name = parts.next()?;
    Some((name, parts.collect()))
}

pub fn lookup(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS
        .iter()
        .find(|spec| spec.name == name || spec.aliases.contains(&name))
}

pub fn run_commands(world: &mut World) {
    let pending = std::mem::take(&mut world.resource_mut::<PendingCommands>().0);
    for command in pending {
        let Some((name, args)) = parse(&command.text) else {
            continue;
        };
        let feedback = match lookup(name) {
            Some(spec) => (spec.run)(world, command.entity, &args),
            None => Err(format!("unknown command: /{name}")),
        };
        let text = match feedback {
            Ok(Some(text)) => text,
            Ok(None) => continue,
            Err(text) => text,
        };
        send_system(world, command.entity, &text);
    }
}

fn send_system(world: &mut World, entity: Entity, text: &str) {
    let message = encode_message(&ServerMessage::System { text: text.into() });
    let mut sessions = world.resource_mut::<Sessions>();
    for session in &mut sessions.sessions {
        if session.entity == Some(entity) && matches!(session.state, SessionState::Playing) {
            session.conn.send(message.clone());
        }
    }
}
