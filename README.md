`nix-update-fetch`
===

Update a call made to a fetcher function call (e.g. package sources) and its surrounding bindings (e.g. `version = "0.1.0";`).

Not meant to be used directly, instead the wrapper script [`nix-upfetch`](https://github.com/msteen/nix-upfetch) that uses both `nix-update-fetch` and [`nix-prefetch`](https://github.com/msteen/nix-prefetch) should be used.

Installation
---

```
git clone https://github.com/msteen/nix-update-fetch.git
cd nix-update-fetch
nix-env --install --file release.nix
```

Help message
---

```
Update a fetcher call

USAGE:
    nix-update-fetch [FLAGS] [OPTIONS] <FETCHER_ARGS> [BINDINGS]

FLAGS:
    -y, --yes        Assume that, yes, you want the changes applied
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -C, --context <CONTEXT>    How much lines of context should be shown at the diff

ARGS:
    <FETCHER_ARGS>    The fetcher arguments to change
    <BINDINGS>        The bindings to change
```
