# spdrs - A simple webcrawler in Rust 🕷️ 🕸️

## Goals

### PoC goals

- [x] Write simple CLI app to fetch a web page from user input
- [x] Extract links using simple strategy, e.g. regex
- [x] Print visited URL and extracted links

### Intermediate goals

- [x] Introduce recursion by fetching extracted links
- [x] Eliminate infinite loops by tracking visited pages
- [ ] Filter external links
- [ ] Fetch in parallel, if not already
- [ ] Firm up validation and error handling

### Stretch goals

- [ ] Parse HTML and extract links from li tags
- [ ] Spin up a local server to host test web pages
- [ ] Write E2E tests against local server
