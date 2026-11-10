# `.stacks/` — tracked container stacks

One directory per stack, committed: a `stack.toml` for `container-compose` plus
whatever script it runs. The `stack.toml` is the whole input — there is no
compose YAML and no second file merged in.

Scratch does not go here. A service writes its artefacts and its build cache
under the owning repository's `.tmp/` (gitignored), so a stack directory stays
small enough to read in one go.

## Layout

```
.stacks/<name>/
  stack.toml        # the stack, commented where it is not obvious
  <job>.sh          # the script the service runs, when it has one
  README.md         # what it proves, how to run it, how to read the result
```

## Rules that are not stylistic

- **`name = "<repo>-<name>"`.** The runtime names containers `<project>-<service>`,
  and a project defaults to the directory name, so two repositories with a
  `linux-check` stack would collide. Prefix the repository.
- **A one-shot job uses `restart = "no"`.** `up -d` is refused when any service
  has a restart policy, and a job that exits is not a service.
- **Do not read `up` as the result.** The runtime records no exit code. A
  service writes its answer to a bind mount and says so in its log; read both.
  `plan` exits 3 when it would refuse and nothing has run, and 2 when the file
  does not validate.
- **Mount the sibling-checkout root, not just this repository,** when the build
  consumes relative `[patch]` paths. `../../../..` from `.stacks/<name>/` is that
  root, relative, so no home directory is baked into the file. Count the levels:
  one short still resolves to an existing directory, so `plan` accepts it and the
  container then cannot find its script.
- **A Linux job sets `CARGO_TARGET_DIR` and `CARGO_HOME` under the repository's
  `.tmp/`,** never the host's `target/`. Cargo does not mix targets across
  platforms, and a bind mount should not either. A container's own filesystem is
  lost on `down`, so an unpinned `CARGO_HOME` re-downloads every crate on every
  run.
- **A verification job uses `--locked`.** Otherwise a container run can rewrite
  a committed lock, and the diff gets attributed to the wrong change.

## Running one

```sh
container system start                                   # once
container-compose -f .stacks/<name>/stack.toml plan
container-compose -f .stacks/<name>/stack.toml up -d
container-compose -f .stacks/<name>/stack.toml logs
container-compose -f .stacks/<name>/stack.toml down
```

`plan` before `up`, every time: it is the only place where a refused plan is
distinguishable from a failure, and exit 3 means nothing ran.

## Stacks here

| stack | what it proves |
|---|---|
| [`linux-check/`](linux-check/) | the Linux backend builds, and a capture renders the same scene as macOS |
