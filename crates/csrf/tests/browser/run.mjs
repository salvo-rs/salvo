import { chromium } from "playwright";
import hostPrefix from "./host_prefix.js";

const executablePath = process.argv[2];
const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : {}) });
try {
  const context = await browser.newContext();
  const result = await hostPrefix(await context.newPage());
  console.log(JSON.stringify({ browser: browser.version(), ...result }));
} finally {
  await browser.close();
}
