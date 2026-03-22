# Verified Bot Commit

[![CI](https://github.com/siiway/verified_bot_commit/actions/workflows/ci.yml/badge.svg)](https://github.com/siiway/verified_bot_commit/actions/workflows/ci.yml)
[![License](https://img.shields.io/github/license/siiway/verified_bot_commit?label=License)](https://github.com/siiway/verified_bot_commit/blob/main/LICENSE)

A GitHub Action to create signed and verified commits as the
`github-actions[bot]` user with the standard `GITHUB_TOKEN`, or with your own
GitHub App Token. Written in Rust for fast, dependency-free execution.

This is accomplished via the GitHub [REST API] by using the [Blob] and [Tree]
endpoints to build the commit and update the original ref to point to it.[^1]

This Action will stage all changed files in your local branch and add those that
match your file patterns to the commit. Afterwards, your local branch will be
updated to point to the newly created commit, which will be signed and verified
using [GitHub's public PGP key](https://github.com/web-flow.gpg). Files that
were not committed by the Action will be left staged.

> [!IMPORTANT]
>
> Using this Action with your own [Personal Access Token (PAT)] is **not**
> recommended.
> See [limitations](#limitations) for more details.

> This action supports Linux, macOS and Windows runners.

## Quick Start

```yaml
- name: Commit changes
  uses: siiway/verified_bot_commit@v0
  with:
    message: 'feat: Some changes'
    files: |
      README.md
      *.txt
      src/**/tests/*
      !test-data/dont-include-this
      test-data/**
```

## Usage

### Inputs

> `List` type is a newline-delimited string
>
> ```yaml
> files: |
>   *.md
>   example.txt
> ```

| Name                 | Type    | Description                                            | Default                    |
| -------------------- | ------- | ------------------------------------------------------ | -------------------------- |
| `repository`         | String  | The target repository [1]                              | `${{ github.repository }}` |
| `ref`                | String  | The ref to push the commit to                          | `${{ github.ref }}`        |
| `files`              | List    | Files/[Glob] patterns to include with the commit [2]   | _required_                 |
| `message`            | String  | Message for the commit [3]                             | _optional_                 |
| `message-file`       | String  | File to use for the commit message [3]                 | _optional_                 |
| `auto-stage`         | Boolean | Stage all changed files for committing [4]             | `true`                     |
| `update-local`       | Boolean | Update local branch after committing [4]               | `true`                     |
| `force-push`         | Boolean | Force push the commit                                  | `false`                    |
| `if-no-commit`       | String  | Set the behavior when no commit is made [5]            | `warning`                  |
| `allow-empty-commit` | Boolean | Allow creating an empty commit if there are no changes | `false`                    |
| `no-throttle`        | Boolean | Disable the throttling mechanism during requests       | `false`                    |
| `no-retry`           | Boolean | Disable the retry mechanism during requests            | `false`                    |
| `max-retries`        | Number  | Number of retries to attempt if a request fails        | `1`                        |
| `follow-symlinks`    | Boolean | Follow symbolic links when globbing files              | `true`                     |
| `workspace`          | String  | Directory containing checked out files                 | `${{ github.workspace }}`  |
| `api-url`            | String  | Base URL for the GitHub API                            | `${{ github.api_url }}`    |
| `token`              | String  | GitHub Token for REST API access [6]                   | `${{ github.token }}`      |

> 1. Must be in the format `owner/repo-name`. To push to other repositories you
>    will _need_ to use a GitHub App Token.
> 2. Files within your `.gitignore` will not be included. You can also negate
>    any files by prefixing it with `!`
> 3. You must include either `message` or `message-file` (which takes priority).
> 4. Only files that match a pattern you include will be in the final commit,
>    but you can optionally stage files yourself for more control.
> 5. Available options are `info`, `notice`, `warning` and `error`. (Will be set
>    to `ignore` if `allow-empty-commit` is `true`)
> 6. This Action is intended to work with the default `GITHUB_TOKEN` or a
>    GitHub App Token. See the [limitations](#limitations) section.

### Outputs

| Name     | Type   | Description                                       |
| -------- | ------ | ------------------------------------------------- |
| `blobs`  | JSON   | A JSON list of blob SHAs within the tree          |
| `tree`   | String | SHA of the underlying tree for the commit         |
| `commit` | String | SHA of the commit itself                          |
| `ref`    | String | SHA for the ref that was updated (same as commit) |

### GITHUB_TOKEN Permissions

This Action requires the following permissions granted to the `GITHUB_TOKEN`.

- `contents: write`

### GitHub App Token

As an alternative to the default `GITHUB_TOKEN`, you can use a GitHub App to
generate the necessary token to create _and_ sign the commit instead. This gives
you a nicely signed/verified commit plus all the benefits that using your own
token provides, such as your own bot's name, writing to protected tags/branches,
writing to other repositories, etc.

## Examples

### Commit all changes

```yaml
- name: Commit & Push changes
  uses: siiway/verified_bot_commit@v0
  with:
    message: 'chore: Updates'
    files: |
      **
```

### Commit changes back to a Pull Request

```yaml
- name: Commit & Push changes
  uses: siiway/verified_bot_commit@v0
  with:
    ref: ${{ github.event.pull_request.head.ref }}
    message: 'chore: Update README'
    files: |
      README.md
```

### Ignore warnings when no files changed

```yaml
- name: Commit & Push changes
  uses: siiway/verified_bot_commit@v0
  with:
    if-no-commit: info
    message: 'feat: Some changes'
    files: |
      README.md
```

### Creating an empty commit

```yaml
- name: Commit & Push changes
  uses: siiway/verified_bot_commit@v0
  with:
    message: 'chore: Empty commit'
    files: '' # don't target any files
    allow-empty-commit: true
```

### Manually stage your own files

```yaml
- name: Stage files
  shell: bash
  run: |
    git add docs/
    git restore --staged docs/something/idont/want

- name: Commit & Push changes
  uses: siiway/verified_bot_commit@v0
  with:
    auto-stage: false
    message: 'chore: Updating docs'
    files: |
      docs/**
```

### Use a GitHub App Token

```yaml
- name: Create GitHub App Token
  uses: actions/create-github-app-token@v2
  id: github-app-token
  with:
    app-id: ${{ secrets.GH_APP_ID }}
    private-key: ${{ secrets.GH_APP_PRIVATE_KEY }}
    owner: ${{ github.repository_owner }}
    repositories: ${{ github.event.repository.name }}

- name: Checkout repository
  uses: actions/checkout@v4
  with:
    ref: ${{ github.event.pull_request.head.ref }}
    token: ${{ steps.github-app-token.outputs.token }}

# Other steps that make changes...

- name: Commit & Push changes
  uses: siiway/verified_bot_commit@v0
  with:
    message: 'chore: Updating README'
    ref: ${{ github.event.pull_request.head.ref }}
    token: ${{ steps.github-app-token.outputs.token }}
    files: |
      README.md
      **/README.md
```

## Limitations

- The default `GITHUB_TOKEN` cannot push to protected refs.
- The [Blob] API has a 40MiB limit; any files larger than this will fail.
- Using your own [Personal Access Token (PAT)] will result in an unsigned and
  unverified commit. Use a GitHub App Token instead, or sign commits yourself
  with tools like [webfactory/ssh-agent](https://github.com/webfactory/ssh-agent)
  and [crazy-max/ghaction-import-gpg](https://github.com/crazy-max/ghaction-import-gpg).

## Development

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable)

### Build

```sh
cargo build
```

### Test

```sh
cargo test
```

### Release

1. Tag a new version:
   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```
2. The [Release](.github/workflows/release.yml) workflow will build binaries for
   all platforms and create a GitHub Release automatically.

## Contributing

Feel free to contribute and make things better by opening an
[Issue](https://github.com/siiway/verified_bot_commit/issues) or
[Pull Request](https://github.com/siiway/verified_bot_commit/pulls).

## License

This project is licensed under the [GNU General Public License v3.0](LICENSE).

## Credits

This is a Rust rewrite of [IAreKyleW00t/verified-bot-commit](https://github.com/IAreKyleW00t/verified-bot-commit),
originally written in TypeScript.

<!-- Links -->

[^1]: [Git Internals - Git Objects](https://git-scm.com/book/en/v2/Git-Internals-Git-Objects)

[REST API]: https://docs.github.com/en/rest
[Personal Access Token (PAT)]: https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens
[Blob]: https://docs.github.com/en/rest/git/blobs
[Tree]: https://docs.github.com/en/rest/git/trees
[Glob]: https://en.wikipedia.org/wiki/Glob_(programming)
