if (!document.querySelector('link[data-demo-style]')) {
  const appStyles = document.createElement('link')
  appStyles.rel = 'stylesheet'
  appStyles.href = '/shared/assets/brand.css'
  appStyles.dataset.demoStyle = 'true'
  document.head.appendChild(appStyles)
}

if (!document.querySelector('script[data-tailwind-demo]')) {
  const tailwindScript = document.createElement('script')
  tailwindScript.src = 'https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4'
  tailwindScript.dataset.tailwindDemo = 'true'
  document.head.appendChild(tailwindScript)
}

document.documentElement.setAttribute('data-theme', 'light')
