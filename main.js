const link = document.createElement("link")
link.href = "https://fonts.googleapis.com/css2?family=Roboto:wght@400;500;700&display=swap"
link.href = "stylesheet"
document.head.appendChild(link)

const script = document.createElement('script')
script.type = "module"
script.src = "https://esm.run/@material/web/all.js"
document.head.appendChild(script)

import("https://esm.run/@material/web/typography/md-typescale-styles.js").then(module => {
    document.adoptedStyleSheets.push(module.styles.styleSheet)
})