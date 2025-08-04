let render: Function;
const routes = new Map<string, Record<string, number>>();
let params: any[] = [];
const slugs: Record<string, any> = {};
let previousRender: string;
let madeRequest: boolean = false;
let elementId: string | null;
const parser = new DOMParser();
let firstRender = routes.size === 0;
const elementEvents: Record<string, EventListener> = {};

if (firstRender) {
  fetch("/routes.json")
    .then((res) => res.json())
    .then((data) => {
      for (const [path, slugs] of Object.entries(data)) {
        routes.set(path, slugs as Record<string, number>);
      }
      setPageTemplate(window.location.pathname);
      firstRender = false;
    });
}

function mergeBodyHTML(html: string) {
  if (html === previousRender) return;

  previousRender = html;

  // document.body.innerHTML = html

  if (madeRequest) {
    document.body.innerHTML = html;
  } else {
    const nextDom = parser.parseFromString(html, "text/html");
    updateDOM(document.body, nextDom.body);
    deleteDOM(document.body, nextDom.body);
    insertDOM(document.body, nextDom.body);
  }

  addEvents();

  if (elementId) {
    const element = document.getElementById(elementId);

    if (element) element.focus();

    elementId = null;
  }
}

function addEvents(elements?: HTMLCollection) {
  if (!elements) elements = document.body.children;

  for (const element of elements) {
    const allEvents: string[] = [];

    for (const attribute of element.attributes) {
      if (attribute.name.startsWith("@")) {
        const event = attribute.name.substring(1);

        allEvents.push(event);

        if (elementEvents[`${element.id}_${event}`]) {
          element.removeEventListener(
            event,
            elementEvents[`${element.id}_${event}`]
          );
        }

        elementEvents[`${element.id}_${event}`] = async function () {
          elementId = element.id;
          mergeBodyHTML(await render(elementId));
        };

        element.addEventListener(
          event,
          elementEvents[`${element.id}_${event}`]
        );
      }

      if (attribute.name.endsWith("ed") && attribute.value === "false")
        element.removeAttribute(attribute.name);
    }

    if (element.hasAttribute("value")) {
      // @ts-ignore
      element.onchange = async (ev) => {
        elementId = element.id;

        mergeBodyHTML(await render(elementId, ev.detail));
      };
    }

    addEvents(element.children);
  }
}

async function onClickEvent(ev: MouseEvent) {
  const target = ev.target as HTMLAnchorElement;
  if (target?.href) {
    const url = new URL(target?.href);
    if (url.origin !== location.origin) return;
    ev.preventDefault();
    setPageTemplate(url.pathname);
  } else mergeBodyHTML(await render());
}

function setPageTemplate(pathname: string) {
  let url;

  try {
    let handler = getHandler(pathname);

    if (handler === "/") handler = "";

    url = `${handler}/HTML.js`;
  } catch (err) {
    url = `/404.js`;
  }

  import(`/pages${url}`).then(async (module) => {
    window.history.replaceState({}, "", pathname);
    render = await module.default();
    madeRequest = true;
    mergeBodyHTML(await render());
    madeRequest = false;
  });
}

function getHandler(pathname: string) {
  let handler;

  if (pathname === "/") return pathname;

  const paths = pathname.split("/").slice(1);

  let bestMatchKey = "";
  let bestMatchLength = -1;

  for (const [routeKey] of routes) {
    const routeSegs = routeKey.split("/");

    const isMatch = pathsMatch(routeSegs, paths.slice(0, routeSegs.length));

    if (isMatch && routeSegs.length > bestMatchLength) {
      bestMatchKey = routeKey;
      bestMatchLength = routeSegs.length;
    }
  }

  if (bestMatchKey) {
    handler = bestMatchKey;

    params = parseParams(paths.slice(bestMatchLength));

    const slugMap = routes.get(bestMatchKey) ?? {};

    Object.entries(slugMap).forEach(([key, idx]) => {
      key = key.replace(":", "");
      slugs[key] = paths[idx];
    });
  }

  if (!handler) throw new Error(`Route ${pathname} not found`, { cause: 404 });

  return handler;
}

function pathsMatch(routeSegs: string[], pathSegs: string[]) {
  if (routeSegs.length !== pathSegs.length) {
    return false;
  }

  const slugs = routes.get(routeSegs.join("/")) || {};

  for (let i = 0; i < routeSegs.length; i++) {
    if (!slugs[routeSegs[i]] && routeSegs[i] !== pathSegs[i]) {
      return false;
    }
  }

  return true;
}

function parseParams(input: string[]) {
  const params = [];

  for (const param of input) {
    const num = Number(param);

    if (!Number.isNaN(num)) params.push(num);
    else if (param === "true") params.push(true);
    else if (param === "false") params.push(false);
    else if (param === "null") params.push(null);
    else if (param === "undefined") params.push(undefined);
    else params.push(param);
  }

  return params;
}

Object.keys(window).forEach((key) => {
  if (/^on/.test(key)) {
    document.addEventListener(key.slice(2), async (ev) => {
      switch (ev.type) {
        case "click":
          await onClickEvent(ev as MouseEvent);
          break;
        case "popstate":
          setPageTemplate(window.location.pathname);
          break;
        default:
          mergeBodyHTML(await render());
          break;
      }
    });
  }
});

/**
 * Updates existing nodes in oldDOM with properties from newDOM
 * @param {Node} oldDOM - The current DOM
 * @param {Node} newDOM - The desired DOM state
 */
function updateDOM(oldDOM: Node, newDOM: Node): void {
  // Skip if nodes are not of the same type
  if (!areNodesOfSameType(oldDOM, newDOM)) {
    return;
  }

  // If it's a text node, update the content
  if (
    oldDOM.nodeType === Node.TEXT_NODE &&
    newDOM.nodeType === Node.TEXT_NODE
  ) {
    if (oldDOM.textContent !== newDOM.textContent) {
      oldDOM.textContent = newDOM.textContent;
    }
    return;
  }

  // If it's an element node, update attributes
  if (
    oldDOM.nodeType === Node.ELEMENT_NODE &&
    newDOM.nodeType === Node.ELEMENT_NODE
  ) {
    // Type assertion since we've already checked nodeType
    const oldElement = oldDOM as Element;
    const newElement = newDOM as Element;

    // Update attributes
    updateAttributes(oldElement, newElement);

    // Recursively update children that exist in both DOMs
    const oldChildren = Array.from(oldDOM.childNodes);
    const newChildren = Array.from(newDOM.childNodes);

    const minLength = Math.min(oldChildren.length, newChildren.length);

    for (let i = 0; i < minLength; i++) {
      updateDOM(oldChildren[i], newChildren[i]);
    }
  }
}

/**
 * Deletes nodes from oldDOM that don't exist in newDOM
 * @param {Node} oldDOM - The current DOM
 * @param {Node} newDOM - The desired DOM state
 */
function deleteDOM(oldDOM: Node, newDOM: Node): void {
  // If nodes are not of the same type, skip (will be handled by insertDOM)
  if (!areNodesOfSameType(oldDOM, newDOM)) {
    return;
  }

  // If it's an element node, check children
  if (
    oldDOM.nodeType === Node.ELEMENT_NODE &&
    newDOM.nodeType === Node.ELEMENT_NODE
  ) {
    const oldChildren = Array.from(oldDOM.childNodes);
    const newChildren = Array.from(newDOM.childNodes);

    // Remove children that exist in oldDOM but not in newDOM
    // Start from the end to avoid index shifting issues
    for (let i = oldChildren.length - 1; i >= newChildren.length; i--) {
      oldDOM.removeChild(oldChildren[i]);
    }

    // Recursively delete for remaining children
    const minLength = Math.min(oldChildren.length, newChildren.length);
    for (let i = 0; i < minLength; i++) {
      deleteDOM(oldChildren[i], newChildren[i]);
    }
  }
}

/**
 * Inserts nodes from newDOM that don't exist in oldDOM
 * @param {Node} oldDOM - The current DOM
 * @param {Node} newDOM - The desired DOM state
 */
function insertDOM(oldDOM: Node, newDOM: Node): void {
  // If nodes are not of the same type, replace the entire node
  if (!areNodesOfSameType(oldDOM, newDOM)) {
    if (oldDOM.parentNode) {
      oldDOM.parentNode.replaceChild(newDOM.cloneNode(true), oldDOM);
    }
    return;
  }

  // If it's an element node, check children
  if (
    oldDOM.nodeType === Node.ELEMENT_NODE &&
    newDOM.nodeType === Node.ELEMENT_NODE
  ) {
    const oldChildren = Array.from(oldDOM.childNodes);
    const newChildren = Array.from(newDOM.childNodes);

    // Add children that exist in newDOM but not in oldDOM
    for (let i = oldChildren.length; i < newChildren.length; i++) {
      oldDOM.appendChild(newChildren[i].cloneNode(true));
    }

    // Recursively insert for existing children
    const minLength = Math.min(oldChildren.length, newChildren.length);
    for (let i = 0; i < minLength; i++) {
      insertDOM(oldChildren[i], newChildren[i]);
    }
  }
}

/**
 * Helper function to update attributes of an element
 * @param {Element} oldElement - The element to update
 * @param {Element} newElement - The element with the desired attributes
 */
function updateAttributes(oldElement: Element, newElement: Element): void {
  // Remove attributes not in newElement
  for (const attr of Array.from(oldElement.attributes)) {
    if (!newElement.hasAttribute(attr.name)) {
      oldElement.removeAttribute(attr.name);
    }
  }

  // Add or update attributes from newElement
  for (const attr of Array.from(newElement.attributes)) {
    // Use strict equality to ensure empty strings are properly handled
    if (
      !attr.name.startsWith("@") &&
      (oldElement.getAttribute(attr.name) !== attr.value ||
        (!oldElement.hasAttribute(attr.name) && attr.value === ""))
    ) {
      if (oldElement.children.length === 0) {
        oldElement.outerHTML = newElement.outerHTML;
      } else oldElement.setAttribute(attr.name, attr.value);
    }
  }
}
/**
 * Helper function to check if two nodes are of the same type
 * @param {Node} oldNode - The old node
 * @param {Node} newNode - The new node
 * @returns {boolean} - Whether the nodes are of the same type
 */
function areNodesOfSameType(oldNode: Node, newNode: Node): boolean {
  // Check if both nodes are defined
  if (!oldNode || !newNode) {
    return false;
  }

  // Check if node types match
  if (oldNode.nodeType !== newNode.nodeType) {
    return false;
  }

  // For element nodes, check if tag names match
  if (
    oldNode.nodeType === Node.ELEMENT_NODE &&
    newNode.nodeType === Node.ELEMENT_NODE
  ) {
    const oldElement = oldNode as Element;
    const newElement = newNode as Element;
    return oldElement.tagName === newElement.tagName;
  }

  return true;
}
