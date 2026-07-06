// @ts-check

// Stateless wrapper page: the sidebar and topic content are components with
// their own re-render scope, so the shell renders once per navigation.
export default class {
  @onMount
  redirectToFirstTopic() {
    if (location.pathname === '/docs' || location.pathname === '/docs/') {
      // The shell itself has no body — land on the first topic. SPA
      // navigation, not location.replace: native bundles have no static
      // file for dynamic topic routes, only the client router knows them.
      /** @type {any} */ (window).Tac.navigate('/docs/introduction')
    }
  }
}
