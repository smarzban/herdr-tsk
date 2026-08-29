import { mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const destDir = join(root, 'public');
const iconLight = join(root, 'public/icon-light.svg');
const iconDark = join(root, 'public/icon-dark.svg');
mkdirSync(destDir, { recursive: true });

async function plate(iconPath, bg, outName) {
  const mark = await sharp(iconPath)
    .resize(153, 150, { fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } })
    .png()
    .toBuffer();
  await sharp({
    create: {
      width: 180,
      height: 180,
      channels: 3,
      background: bg,
    },
  })
    .composite([{ input: mark, gravity: 'centre' }])
    .png()
    .toFile(join(destDir, outName));
}

await plate(iconLight, '#E8E4D4', 'apple-touch-icon-light.png');
await plate(iconDark, '#111111', 'apple-touch-icon-dark.png');
await plate(iconDark, '#111111', 'apple-touch-icon.png');
