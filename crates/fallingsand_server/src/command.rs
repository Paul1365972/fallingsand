use crate::player::Players;
use fallingsand_core::{Calendar, DAY_UNITS};
use fallingsand_protocol::{ChatEntry, CommandInfo, GameMode, ParamKind, ParamSpec, PlayerId};
use std::sync::LazyLock;

pub enum Reply {
    To(PlayerId, ChatEntry),
    All(ChatEntry),
}

pub struct World<'a> {
    pub players: &'a mut Players,
    pub clock: &'a mut Calendar,
}

pub struct Ctx<'a, 'w> {
    world: &'a mut World<'w>,
    caller: PlayerId,
    replies: &'a mut Vec<Reply>,
}

impl Ctx<'_, '_> {
    fn reply(&mut self, text: impl Into<String>) {
        self.replies
            .push(Reply::To(self.caller, ChatEntry::system(text)));
    }

    fn announce(&mut self, text: impl Into<String>) {
        self.replies.push(Reply::All(ChatEntry::announce(text)));
    }

    fn fail(&mut self, text: impl Into<String>) {
        self.replies
            .push(Reply::To(self.caller, ChatEntry::error(text)));
    }
}

pub enum Error {
    Usage,
    Failed(String),
}

impl From<&str> for Error {
    fn from(text: &str) -> Self {
        Error::Failed(text.to_string())
    }
}

impl From<String> for Error {
    fn from(text: String) -> Self {
        Error::Failed(text)
    }
}

pub struct Args<'a>(std::slice::Iter<'a, &'a str>);

impl<'a> Args<'a> {
    fn next(&mut self) -> Result<&'a str, Error> {
        self.0.next().copied().ok_or(Error::Usage)
    }

    fn optional(&mut self) -> Option<&'a str> {
        self.0.next().copied()
    }

    fn end(&mut self) -> Result<(), Error> {
        match self.0.next() {
            Some(_) => Err(Error::Usage),
            None => Ok(()),
        }
    }
}

type Run = fn(&mut Ctx, Args) -> Result<(), Error>;

pub struct Command {
    info: CommandInfo,
    run: Run,
}

impl Command {
    fn alias(mut self, alias: &str) -> Self {
        self.info.aliases.push(alias.to_string());
        self
    }
}

fn command(name: &str, summary: &str, params: Vec<ParamSpec>, run: Run) -> Command {
    Command {
        info: CommandInfo {
            name: name.to_string(),
            aliases: Vec::new(),
            summary: summary.to_string(),
            params,
        },
        run,
    }
}

static COMMANDS: LazyLock<Vec<Command>> = LazyLock::new(|| {
    vec![
        command(
            "help",
            "list commands, or show one command's usage",
            vec![ParamSpec::new("command", ParamKind::Command, &[]).optional()],
            help,
        )
        .alias("?"),
        command(
            "gamemode",
            "switch your game mode",
            vec![ParamSpec::new(
                "mode",
                ParamKind::Choice,
                &["survival", "creative"],
            )],
            gamemode,
        )
        .alias("gm"),
        command(
            "time",
            "set the world clock",
            vec![ParamSpec::new(
                "when",
                ParamKind::Free,
                &["day", "night", "noon", "midnight"],
            )],
            time,
        ),
    ]
});

fn lookup(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.info.matches(name))
}

pub fn table() -> Vec<CommandInfo> {
    COMMANDS
        .iter()
        .map(|command| command.info.clone())
        .collect()
}

pub fn run(world: &mut World) -> Vec<Reply> {
    let pending: Vec<(PlayerId, String)> = world
        .players
        .iter_mut()
        .flat_map(|(&id, player)| {
            std::mem::take(&mut player.control.pending_commands)
                .into_iter()
                .map(move |line| (id, line))
        })
        .collect();
    let mut replies = Vec::new();
    for (caller, line) in pending {
        if !world
            .players
            .get(caller)
            .is_some_and(|player| player.is_alive())
        {
            continue;
        }
        dispatch(
            &mut Ctx {
                world,
                caller,
                replies: &mut replies,
            },
            &line,
        );
    }
    replies
}

fn dispatch(ctx: &mut Ctx, line: &str) {
    let mut parts = line.split_whitespace();
    let Some(name) = parts.next() else {
        return;
    };
    let parts: Vec<&str> = parts.collect();
    let Some(command) = lookup(name) else {
        ctx.fail(format!("unknown command: /{name}"));
        return;
    };
    if let Err(error) = (command.run)(ctx, Args(parts.iter())) {
        match error {
            Error::Usage => ctx.fail(format!("usage: {}", command.info.usage())),
            Error::Failed(text) => ctx.fail(text),
        }
    }
}

fn describe(command: &Command) -> String {
    format!("{} - {}", command.info.usage(), command.info.summary)
}

fn help(ctx: &mut Ctx, mut args: Args) -> Result<(), Error> {
    let target = args.optional();
    args.end()?;
    match target {
        None => {
            for command in COMMANDS.iter() {
                ctx.reply(describe(command));
            }
        }
        Some(name) => {
            let name = name.trim_start_matches('/');
            let command = lookup(name).ok_or_else(|| format!("unknown command: /{name}"))?;
            ctx.reply(describe(command));
        }
    }
    Ok(())
}

fn gamemode(ctx: &mut Ctx, mut args: Args) -> Result<(), Error> {
    let mode = GameMode::parse(args.next()?).ok_or(Error::Usage)?;
    args.end()?;
    let caller = ctx.caller;
    let player = ctx
        .world
        .players
        .get_mut(caller)
        .ok_or("player not in world")?;
    if player.profile.mode == mode {
        ctx.reply(format!("already in {} mode", mode.label()));
        return Ok(());
    }
    player.profile.mode = mode;
    if mode != GameMode::Creative
        && let Some(avatar) = player.avatar_mut()
    {
        avatar.flying = false;
    }
    ctx.reply(format!("game mode set to {}", mode.label()));
    Ok(())
}

fn time(ctx: &mut Ctx, mut args: Args) -> Result<(), Error> {
    let when = args.next()?;
    args.end()?;
    let day = ctx.world.clock.day();
    ctx.world.clock.age = match when {
        "day" | "noon" => day * DAY_UNITS + DAY_UNITS / 2,
        "night" | "midnight" => day * DAY_UNITS,
        text => {
            let target: f64 = text
                .parse()
                .ok()
                .filter(|day: &f64| day.is_finite() && *day >= 0.0)
                .ok_or(Error::Usage)?;
            (target * DAY_UNITS as f64) as u64
        }
    };
    let (day, minute) = (ctx.world.clock.day(), ctx.world.clock.minute_of_day());
    ctx.announce(format!(
        "time set to {:02}:{:02} of day {day}",
        minute / 60,
        minute % 60
    ));
    Ok(())
}
