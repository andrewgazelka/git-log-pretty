# git-log-pretty

A pretty git log viewer for showing commits ahead of main, built with Rust. On a
graphics-capable terminal it draws each commit author's GitHub avatar inline
using the [kitty graphics protocol].

```bash
nix run github:andrewgazelka/git-log-pretty
```

## Avatars

Each commit is shown with the author's GitHub avatar in the left gutter. The
author's GitHub login is resolved cheapest-first:

1. `…@users.noreply.github.com` commit emails carry the login directly (no
   network).
2. Otherwise the commit is looked up via the GitHub API for `origin`, which
   resolves any email linked to a GitHub account.
3. Failing that, the email is searched in GitHub's public user index.

Avatars are downloaded from `https://github.com/<login>.png`, cached under
`$XDG_CACHE_HOME/git-log-pretty/avatars`, and only transmitted once per author
per run. The API lookups use a token from `GITHUB_TOKEN`, `GH_TOKEN`, or
`gh auth token` when available; the avatar download itself needs no token.

If your commit email is not linked to your GitHub account, map it yourself:

```bash
git config --add githubLogin.map "you@example.com=your-login"
```

Flags: `--no-avatar` disables the images; `--avatar-rows N` sets their height in
terminal rows (default `2`, `0` also disables). Avatars are skipped automatically
when output is not a kitty/ghostty/wezterm terminal or is piped to a file.

## Workspace

- [`kitty`](crates/kitty) — encoder for the kitty graphics protocol (no image
  decoding, no terminal I/O).
- [`github`](crates/github) — resolve a commit author to a GitHub user and fetch
  their avatar.
- [`git-log-pretty`](crates/git-log-pretty) — the CLI that ties them together.

[kitty graphics protocol]: https://sw.kovidgoyal.net/kitty/graphics-protocol/
