#!/usr/bin/env node
// Thin launcher for NIKI native binary.
// Resolves the prebuilt native binary from optionalDependencies or local install.

import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));

function resolveBinary() {
  const platform = process.platform;
  const arch = process.arch;

  const pkgMap = {
    'linux-x64': '@niki-cli/linux-x64',
    'linux-arm64': '@niki-cli/linux-arm64',
    'darwin-x64': '@niki-cli/darwin-x64',
    'darwin-arm64': '@niki-cli/darwin-arm64',
    'win32-x64': '@niki-cli/win32-x64',
  };

  const key = `${platform}-${arch}`;
  const binName = platform === 'win32' ? 'niki.exe' : 'niki';

  // 1. Try local node_modules vendor path
  const localVendor = join(__dirname, '..', 'vendor', binName);
  if (existsSync(localVendor)) {
    return localVendor;
  }

  // 2. Try optional dependency package
  const pkgName = pkgMap[key];
  if (pkgName) {
    try {
      const pkgPath = import.meta.resolve ? import.meta.resolve(pkgName) : null;
      if (pkgPath) {
        const candidate = fileURLToPath(new URL(`bin/${binName}`, pkgPath));
        if (existsSync(candidate)) return candidate;
      }
    } catch {
      // Optional dependency not installed
    }
  }

  // 3. Fallback: check PATH
  return 'niki';
}

const binary = resolveBinary();
const child = spawn(binary, process.argv.slice(2), {
  stdio: 'inherit',
  env: process.env,
});

child.on('error', (err) => {
  if (err.code === 'ENOENT') {
    console.error(
      'error: niki native binary not found for your platform.\n' +
        'Please install via curl/PowerShell installer:\n' +
        '  curl -fsSL https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.sh | bash\n'
    );
  } else {
    console.error(`error launching niki: ${err.message}`);
  }
  process.exit(1);
});

child.on('close', (code) => {
  process.exit(code ?? 0);
});
