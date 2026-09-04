import { copyFileSync, mkdirSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const website = join(dirname(fileURLToPath(import.meta.url)), '..');
const root = join(website, '..');
const pub = join(website, 'public');
mkdirSync(pub, { recursive: true });
copyFileSync(join(root, 'install.sh'), join(pub, 'install.sh'));
copyFileSync(join(root, 'docs', 'logo.png'), join(pub, 'logo.png'));
for (const name of readdirSync(join(root, 'docs', 'favicons'))) {
  copyFileSync(join(root, 'docs', 'favicons', name), join(pub, name));
}
