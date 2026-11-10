// Run this function with a Playwright Page in a fresh browser context.
// All requests are fulfilled locally; the .test names never reach a real service.
export default async function hostPrefix(page) {
  const context = page.context();
  await context.clearCookies();
  let submitted;
  await page.route("https://*.csrf.test/**", async route => {
    const url = new URL(route.request().url());
    if (url.pathname === "/start") {
      await route.fulfill({status: 200, headers: {"content-type": "text/html", "set-cookie":
        "__Host-salvo.csrf=victim; Secure; HttpOnly; SameSite=Strict; Path=/"}, body: "victim"});
    } else if (url.pathname === "/inject") {
      await route.fulfill({status: 200, headers: {"content-type": "text/html", "set-cookie":
        "__Host-salvo.csrf=attacker; Secure; Domain=csrf.test; Path=/"}, body: "injection"});
    } else if (url.pathname === "/encoded-alias") {
      await route.fulfill({status: 200, headers: {"content-type": "text/html", "set-cookie":
        "%5F_Host-salvo.csrf=encoded-attacker; Secure; SameSite=Strict; Domain=csrf.test; Path=/"}, body: "encoded alias"});
    } else if (url.pathname === "/legacy") {
      await route.fulfill({status: 200, headers: {"content-type": "text/html", "set-cookie":
        "salvo.csrf=attacker; Secure; SameSite=Strict; Domain=csrf.test; Path=/action"}, body: "legacy"});
    } else if (url.pathname === "/own") {
      await route.fulfill({status: 200, headers: {"content-type": "text/html", "set-cookie":
        "__Host-salvo.csrf=attacker-own; Secure; Path=/"}, body: "own"});
    } else if (url.pathname === "/bad-path") {
      await route.fulfill({status: 200, headers: {"content-type": "text/html", "set-cookie":
        "__Host-invalid=bad; Secure; Path=/nested"}, body: "bad path"});
    } else if (url.pathname === "/bad-secure") {
      await route.fulfill({status: 200, headers: {"content-type": "text/html", "set-cookie":
        "__Host-insecure=bad; Path=/"}, body: "insecure"});
    } else if (url.pathname === "/action") {
      submitted = {method: route.request().method(), headers: await route.request().allHeaders(), body: route.request().postData()};
      await route.fulfill({status: 200, body: "submitted"});
    } else {
      await route.fulfill({status: 200, headers: {"content-type": "text/html"}, body: "ready"});
    }
  });
  await page.goto("https://app.csrf.test/start");
  await page.goto("https://attacker.csrf.test/inject");
  await page.goto("https://attacker.csrf.test/encoded-alias");
  await page.goto("https://attacker.csrf.test/legacy");
  await page.goto("https://attacker.csrf.test/own");
  await page.goto("https://attacker.csrf.test/bad-path");
  await page.goto("https://attacker.csrf.test/bad-secure");
  const cookies = await context.cookies("https://app.csrf.test/action");
  const host = cookies.filter(c => c.name === "__Host-salvo.csrf");
  if (host.length !== 1 || host[0].value !== "victim" || host[0].domain !== "app.csrf.test") throw Error("host cookie overwritten");
  if (!cookies.some(c => c.name === "%5F_Host-salvo.csrf" && c.value === "encoded-attacker")) throw Error("encoded alias control failed");
  if (!cookies.some(c => c.name === "salvo.csrf" && c.value === "attacker")) throw Error("legacy injection control failed");
  if ((await context.cookies()).some(c => c.name === "__Host-invalid" || c.name === "__Host-insecure")) throw Error("invalid prefix accepted");
  await page.evaluate(() => {
    const form = document.createElement("form");
    form.method = "POST";
    form.action = "https://app.csrf.test/action";
    const token = document.createElement("input");
    token.name = "csrf"; token.value = "attacker"; form.append(token);
    document.body.append(form); form.submit();
  });
  await page.waitForURL("https://app.csrf.test/action");
  const cookie = submitted.headers.cookie || "";
  if (submitted.method !== "POST" || !cookie.includes("__Host-salvo.csrf=victim") || !cookie.includes("salvo.csrf=attacker") || !cookie.includes("%5F_Host-salvo.csrf=encoded-attacker")) throw Error("same-site form control failed");
  return {encodedParentDomainAliasIsDistinct: true, hostCookiePreserved: true, parentDomainInjectionRejected: true, invalidAttributesRejected: true, legacyInjectionControl: true, sameSiteFormCarriesVictimHostCookie: true};
}
