// Shoelace web component library — loaded via CDN, zero build step
const link = document.createElement("link");
link.rel = "stylesheet";
link.href = "https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.20.1/cdn/themes/light.css";
document.head.appendChild(link);

const darkLink = document.createElement("link");
darkLink.rel = "stylesheet";
darkLink.href = "https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.20.1/cdn/themes/dark.css";
document.head.appendChild(darkLink);

const script = document.createElement("script");
script.type = "module";
script.src = "https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.20.1/cdn/shoelace-autoloader.js";
document.head.appendChild(script);

document.documentElement.setAttribute("data-theme", "light");