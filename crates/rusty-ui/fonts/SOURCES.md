# Font sources

## Inter 4.1

Release: https://github.com/rsms/inter/releases/tag/v4.1

Archive: https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip

Static TTFs are copied unchanged from `extras/ttf/`. `Inter-OFL.txt` is the unchanged `LICENSE.txt`.

## Space Grotesk 2.0.0

Release: https://github.com/floriankarsten/space-grotesk/releases/tag/2.0.0

Archive: https://github.com/floriankarsten/space-grotesk/releases/download/2.0.0/SpaceGrotesk-2.0.0.zip

`SpaceGrotesk-OFL.txt` is copied unchanged from `SpaceGrotesk-2.0.0/OFL.txt`.
The release license declares no Reserved Font Name after its copyright statement.

Variable source: `SpaceGrotesk-2.0.0/ttf/SpaceGrotesk[wght].ttf`.

Variable SHA-256: `acad6de1fc93436f5c0f1f4137751ef04f1aea3063e7036535970ffcfbd79f72`.

fontTools version: `4.60.1`, installed from PyPI in `target/fonttools-venv`.

Exact command, run from the repository root:

```sh
target/fonttools-venv/bin/python -m fontTools.varLib.instancer 'target/font-f1-21q29dwj/SpaceGrotesk[wght].ttf' wght=600 -o target/font-f1-21q29dwj/SpaceGrotesk-SemiBold.ttf
```

The output is copied unchanged to `SpaceGrotesk-SemiBold.ttf`.

## Bundled files

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| Inter-Regular.ttf | 411640 | `40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82` |
| Inter-Medium.ttf | 417300 | `97ad806f526e41546d46365bb3a393145f75b7b1568913db74549ad8b8dba872` |
| Inter-SemiBold.ttf | 419744 | `78a843fade9d4612a5567302fb595b56976eb5fcebf4fea5a5912d638bafcde3` |
| Inter-Bold.ttf | 420428 | `288316099b1e0a47a4716d159098005eef7c0066921f34e3200393dbdb01947f` |
| Inter-OFL.txt | 4380 | `262481e844521b326f5ecd053e59b98c8b2da78c8ee1bdbb6e8174305e54935a` |
| SpaceGrotesk-SemiBold.ttf | 88216 | `60c0d9346ebebc1bfef8ef6a7aad68c49aea95635c15488e2c546918ff4a0a58` |
| SpaceGrotesk-OFL.txt | 4495 | `564ce565c371c5e5bbf286006565a7c9aa55a9f56e7ca58d56e05d649dd61a72` |
