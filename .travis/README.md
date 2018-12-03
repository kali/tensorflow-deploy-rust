# Travis & minions test infrastructure

## Principles

* travis is triggered on each commit, it will run `./.travis/native.sh` to
    perform x86_64 builds, plus a series of `./.travis/cross.sh` for as many
    arm boards configurations.
* `.travis/cross.sh` pushes a `.tgz` to a s3 bucket for each configuration. The
    bundle contains a `entrypoint.sh` script and anything it depends on,
    including the relevant `tract` cli executable. The script is actually names
    `bundle-entrypoint.sh` in the repository.
* devices are running `minion.sh` and will pick the new bundles from the s3 bucket,
    untar and run the `entrypoint.sh`

## Testing locally

```
cargo build --release -p tract && .travis/run-bundle.sh `.travis/make_bundle.sh`
```

## minion setup

```
MINION=user@hostname.local
scp .travis/minionrc $MINION:.minionrc
scp .travis/minion.sh $MINION:
```

```
apt install wget curl perl
```

On device: `.minioncrc` set a MINION_ID. At this point, running `./minion.sh`
should work.

`crontab -e`

```
*/10 * * * * $HOME/minion.sh
```
