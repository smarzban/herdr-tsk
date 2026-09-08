import { existsSync } from 'node:fs';
import { defineConfig } from '@playwright/test';
export default defineConfig({
 testDir: './parity', testMatch: '*.spec.mjs', fullyParallel: false, workers: 1,
 outputDir: './parity-results', reporter: 'list',
 use: { baseURL:'http://127.0.0.1:4178', viewport:{width:1400,height:1000},
  launchOptions: { executablePath: process.env.PARITY_CHROME || (existsSync('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome') ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' : undefined) } },
 webServer: { command:'node parity/server.mjs', url:'http://127.0.0.1:4178', reuseExistingServer:false },
});
