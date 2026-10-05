# niki-cli

Thin npm launcher for [NIKI](https://github.com/RavaniRoshan/niki).

## Installation

```bash
npm install -g niki-cli
# or run directly
npx niki-cli --help
```

## How It Works

This launcher package delegates execution to the platform-specific precompiled binary `@niki-cli/<platform>` installed via `optionalDependencies` with zero postinstall scripts.
