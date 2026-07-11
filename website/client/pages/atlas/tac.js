// @ts-check

// Stateless wrapper page (docs pattern): the sidebar and section content are
// pages/components with their own re-render scope, so the shell renders once
// per navigation.
export default class {
  constructor() {
    if (typeof document !== 'undefined') document.title = 'Capability atlas — Tachyon'
  }

  @onMount
  redirectToOverview() {
    if (location.pathname === '/atlas' || location.pathname === '/atlas/') {
      // The shell itself has no body — land on the overview section. SPA
      // navigation, not location.replace: section pages are real static
      // routes, but in-app navigation keeps the shell warm.
      /** @type {any} */ (window).Tac.navigate('/atlas/overview')
    }
  }
}
