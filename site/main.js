const status = document.querySelector("#status");

try {
  const { mount } = await import("./host.js");
  const app = await mount(document.querySelector("canvas"), new URL("./blit_site.wasm", import.meta.url));
  const route = () => {
    const page = { "#comparisons": 1, "#evolution": 2 }[location.hash] ?? 0;
    app.exports.set_page(page);
    document.title = ["Blit — less machinery. More machine.", "Comparisons — Blit", "API evolution — Blit"][page];
    window.scrollTo(0, 0);
    app.draw();
  };
  window.addEventListener("hashchange", route);
  route();
  status.hidden = true;
} catch (error) {
  status.replaceChildren();
  const title = document.createElement("strong");
  title.textContent = "The interface couldn’t start.";
  const message = document.createElement("p");
  message.textContent = "Please reload in a browser with WebAssembly and Canvas 2D support.";
  const source = document.createElement("a");
  source.href = "https://github.com/nicoburniske/blit";
  source.textContent = "Read the source and documentation ↗";
  status.append(title, message, source);
  console.error(error);
}
