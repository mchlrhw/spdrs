# spdrs - A simple webcrawler in Rust 🕷️ 🕸️

## Goals

### PoC goals

- [ ] Write simple CLI app to fetch a web page from user input
- [ ] Extract links using simple strategy, e.g. regex
- [ ] Print visited URL and extracted links

### Intermediate goals

- [ ] Introduce recursion by fetching extracted links
- [ ] Eliminate infinite loops by tracking visited pages
- [ ] Fetch in parallel, if not already
- [ ] Firm up validation and error handling

### Stretch goals

- [ ] Parse HTML and extract links from li tags
- [ ] Spin up a local server to host test web pages
- [ ] Write E2E tests against local server
