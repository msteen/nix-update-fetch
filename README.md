`nix-update-fetch`
===

Update a call made to a Nix fetcher, i.e. a package source.

Works together with:
https://github.com/msteen/nix-prefetch

Installation
---

```
git clone https://github.com/msteen/nix-update-fetch.git
cd nix-update-fetch
nix-env --install --file release.nix
```

Features
---

* Can update most packages as in the example, see Limitations.
* Can work with string interpolated bindings, see Examples.
* Aside from fetcher arguments a version can be supplied, so that its binding will also be modified.
* Can handle `${majorMinor version}` in an URL.

Limitations
---

* Cannot handle inheriting fetcher arguments from an expression, i.e. `inherit (args) sha256;` will fail, but `inherit sha256;` works (see next point).
* Can only handle attribute set and let bindings, so it cannot handle `with` expressions or function arguments at the moment.

Examples
---

Can handle interpolated bindings and simple inherits:

```
{ stdenv, fetchurl }:

let
  sha256 = "0000000000000000000000000000000000000000000000000000";
  rev = "112c7d23f90da692927b76f7284c8047e50fdc14";

in stdenv.mkDerivation rec {
  name = "${pname}-${version}";
  pname = "test";
  version = "0.1.0";

  src = fetchurl {
    inherit sha256;
    url = "https://gist.githubusercontent.com/msteen/fef0b259aa8e26e9155fa0f51309892c/raw/${rev}/test.txt";
  };
}
```

```
$ nix-update-fetch "$(nix-prefetch test --quiet --output json --with-position --diff
  --url https://gist.githubusercontent.com/msteen/fef0b259aa8e26e9155fa0f51309892c/raw/98170052fc54d3e901cca0d7d4a68e1424a58e94/test.txt)"

 let
-  rev = "112c7d23f90da692927b76f7284c8047e50fdc14";
-  sha256 = "0000000000000000000000000000000000000000000000000000";
+  rev = "98170052fc54d3e901cca0d7d4a68e1424a58e94";
+  sha256 = "0ddb2gn6wrisva81zidnv03rq083bndqnwar7zhfw5jy4qx5wwyl";
 in

Do you want to apply these changes?
```

Update a GitHub revision:

```
$ nix-update-fetch "$(nix-prefetch openraPackages.mods.ca --index 0 --quiet --output json --with-position --diff --rev master)"
       owner = "Inq8";
       repo = "CAmod";
-      rev = "16fb77d037be7005c3805382712c33cec1a2788c";
-      sha256 = "11fjyr3692cy2a09bqzk5ya1hf6plh8hmdrgzds581r9xbj0q4pr";
+      rev = "master";
+      sha256 = "15w91xs253gyrlzsgid6ixxjazx0fbzick6vlkiay0znb58n883m";
     };
     engine = let commit = "b8a7dd52ff893ed8225726d4ed4e14ecad748404"; in {
Do you want to apply these changes?
```

Help message
---

```
Update a fetcher call

USAGE:
    nix-update-fetch [OPTIONS] <FETCHER_ARGS>

FLAGS:
    -h, --help    Prints help information

OPTIONS:
    -C, --context <CONTEXT>    How much lines of context should be shown at the diff
    -v, --version <VERSION>    Change the version regardless of it being used in the fetcher arguments

ARGS:
    <FETCHER_ARGS>    The fetcher arguments to change
```
