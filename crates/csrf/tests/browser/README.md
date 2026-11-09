# Browser cookie isolation regression

```sh
cd crates/csrf/tests/browser
npm install
npx playwright install chromium
npm test
```

An already installed Chromium executable may instead be passed as
`npm test -- /absolute/path/to/chrome`. All HTTPS requests for app/attacker
subdomains of `csrf.test` are intercepted and fulfilled locally. The test
uses real browser cookie storage and a same-site cross-origin form POST;
it does not contact an external server or depend on DNS/hosts-file changes.

Checks include a rejected parent-domain host-prefix injection, an accepted
legacy-cookie injection control, a sibling's separate host-only cookie,
invalid Path/Secure rejection, and the cookies sent by the forged form.
This covers browser cookie scope; Rust tests separately verify the store's
actual cookie attributes and server-side rejection of invalid configuration.

The browser also accepts a sibling's percent-encoded `%5F_Host-salvo.csrf` parent-domain
cookie as a distinct, unprefixed name and sends it alongside the genuine cookie. This is
an injection control, not a browser prefix violation. The Rust Request-parser regression
in `host_prefix_uses_original_cookie_names_not_decoded_jar_aliases` verifies that
CookieStore selects the original exact name even when the general decoded cookie jar
has been shadowed by this alias. Duplicate exact names fail closed.
