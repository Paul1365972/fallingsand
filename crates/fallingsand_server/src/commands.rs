use crate::player::Players;
use fallingsand_core::{Calendar, DAY_UNITS};
use fallingsand_protocol::{GameMode, PlayerId};

#[derive(Clone, Copy)]
enum Command {
    Help,
    Gamemode,
    Time,
}

struct CommandSpec {
    command: Command,
    name: &'static str,
    aliases: &'static [&'static str],
    usage: &'static str,
}

const HELP: CommandSpec = CommandSpec {
    command: Command::Help,
    name: "help",
    aliases: &["?"],
    usage: "/help [command]",
};
const GAMEMODE: CommandSpec = CommandSpec {
    command: Command::Gamemode,
    name: "gamemode",
    aliases: &["gm"],
    usage: "/gamemode <survival|creative|s|c>",
};
const TIME: CommandSpec = CommandSpec {
    command: Command::Time,
    name: "time",
    aliases: &[],
    usage: "/time <day|night|noon|midnight|DAY>",
};
const COMMANDS: &[CommandSpec] = &[HELP, GAMEMODE, TIME];

pub fn run_commands(players: &mut Players, clock: &mut Calendar) -> Vec<(PlayerId, String)> {
    let pending: Vec<_> = players
        .iter_mut()
        .flat_map(|(&id, player)| {
            std::mem::take(&mut player.control.pending_commands)
                .into_iter()
                .map(move |text| (id, text))
        })
        .collect();
    let mut feedback = Vec::new();
    for (player, text) in pending {
        if !players.get(player).is_some_and(|player| player.is_alive()) {
            continue;
        }
        let Some((name, args)) = parse(&text) else {
            continue;
        };
        let result = match lookup(name).map(|spec| spec.command) {
            Some(Command::Help) => run_help(&args),
            Some(Command::Gamemode) => run_gamemode(players, player, &args),
            Some(Command::Time) => run_time(clock, &args),
            None => Err(format!("unknown command: /{name}")),
        };
        match result {
            Ok(Some(text)) | Err(text) => feedback.push((player, text)),
            Ok(None) => {}
        }
    }
    feedback
}

fn run_help(args: &[&str]) -> Result<Option<String>, String> {
    match args {
        [] => Ok(Some(
            COMMANDS
                .iter()
                .map(|command| command.usage)
                .collect::<Vec<_>>()
                .join("\n"),
        )),
        [name] => lookup(name)
            .map(|command| Some(command.usage.to_string()))
            .ok_or_else(|| format!("unknown command: /{name}")),
        _ => Err(format!("usage: {}", HELP.usage)),
    }
}

fn run_gamemode(
    players: &mut Players,
    player: PlayerId,
    args: &[&str],
) -> Result<Option<String>, String> {
    let mode = match args {
        [arg] => GameMode::parse(arg),
        _ => None,
    }
    .ok_or_else(|| format!("usage: {}", GAMEMODE.usage))?;
    let player = players
        .get_mut(player)
        .ok_or_else(|| "player not in world".to_string())?;
    if player.profile.mode == mode {
        return Ok(Some(format!("already in {} mode", mode.label())));
    }
    player.profile.mode = mode;
    if mode != GameMode::Creative
        && let Some(avatar) = player.avatar_mut()
    {
        avatar.flying = false;
    }
    Ok(Some(format!("game mode set to {}", mode.label())))
}

fn run_time(clock: &mut Calendar, args: &[&str]) -> Result<Option<String>, String> {
    let [arg] = args else {
        return Err(format!("usage: {}", TIME.usage));
    };
    let day = clock.day();
    match *arg {
        "day" | "noon" => clock.age = day * DAY_UNITS + DAY_UNITS / 2,
        "night" | "midnight" => clock.age = day * DAY_UNITS,
        arg => {
            let target: f64 = arg
                .parse()
                .ok()
                .filter(|day: &f64| day.is_finite() && *day >= 0.0)
                .ok_or_else(|| format!("usage: {}", TIME.usage))?;
            clock.age = (target * DAY_UNITS as f64) as u64;
        }
    }
    let (day, minute) = (clock.day(), clock.minute_of_day());
    Ok(Some(format!(
        "time set to {:02}:{:02} of day {day}",
        minute / 60,
        minute % 60
    )))
}

fn parse(text: &str) -> Option<(&str, Vec<&str>)> {
    let text = text.strip_prefix('/')?;
    let mut parts = text.split_whitespace();
    let name = parts.next()?;
    Some((name, parts.collect()))
}

fn lookup(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS
        .iter()
        .find(|spec| spec.name == name || spec.aliases.contains(&name))
}
