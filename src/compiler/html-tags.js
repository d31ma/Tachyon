// @ts-check

/** HTML and SVG element names accepted by Tac's template parser. */
export const HTML_ELEMENT_NAMES = Object.freeze([
    'a', 'abbr', 'address', 'area', 'article', 'aside', 'audio', 'b', 'base', 'bdi', 'bdo',
    'blockquote', 'body', 'br', 'button', 'canvas', 'caption', 'cite', 'code', 'col',
    'colgroup', 'data', 'datalist', 'dd', 'del', 'details', 'dfn', 'dialog', 'div', 'dl',
    'dt', 'em', 'embed', 'fieldset', 'figcaption', 'figure', 'footer', 'form', 'h1', 'h2',
    'h3', 'h4', 'h5', 'h6', 'head', 'header', 'hgroup', 'hr', 'html', 'i', 'iframe', 'img',
    'input', 'ins', 'kbd', 'label', 'legend', 'li', 'link', 'main', 'map', 'mark', 'menu',
    'meta', 'meter', 'nav', 'noscript', 'object', 'ol', 'optgroup', 'option', 'output', 'p',
    'picture', 'pre', 'progress', 'q', 'rp', 'rt', 'ruby', 's', 'samp', 'script', 'search',
    'section', 'select', 'slot', 'small', 'source', 'span', 'strong', 'style', 'sub',
    'summary', 'sup', 'table', 'tbody', 'td', 'template', 'textarea', 'tfoot', 'th', 'thead',
    'time', 'title', 'tr', 'track', 'u', 'ul', 'var', 'video', 'wbr',
    'svg', 'g', 'path', 'circle', 'ellipse', 'line', 'polygon', 'polyline', 'rect', 'defs',
    'lineargradient', 'radialgradient', 'stop', 'symbol', 'use', 'clippath', 'mask', 'text',
    'tspan', 'foreignobject', 'animate', 'animatemotion', 'animatetransform', 'pattern',
    'marker', 'filter', 'feblend', 'fecolormatrix', 'fecomponenttransfer', 'fecomposite',
    'feconvolvematrix', 'fediffuselighting', 'fedisplacementmap', 'fedistantlight',
    'fedropshadow', 'feflood', 'fefunca', 'fefuncb', 'fefuncg', 'fefuncr', 'fegaussianblur',
    'feimage', 'femerge', 'femergenode', 'femorphology', 'feoffset', 'fepointlight',
    'fespecularlighting', 'fespotlight', 'fetile', 'feturbulence',
]);

export const HTML_ELEMENT_SET = new Set(HTML_ELEMENT_NAMES);
export const TAC_CONTROL_ELEMENT_SET = new Set(['loop', 'logic', 'switch', 'case']);

/** Elements with a schema-v1 mapping on every native UI target. */
export const NATIVE_UI_ELEMENT_SET = new Set([
    'main', 'div', 'section', 'article', 'header', 'footer', 'nav', 'aside', 'form',
    'fieldset', 'figure', 'figcaption', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
    'p', 'span', 'label', 'strong', 'b', 'em', 'i', 'small', 'code', 'pre',
    'blockquote', 'button', 'input', 'ul', 'ol', 'li', 'table', 'thead', 'tbody',
    'tfoot', 'tr', 'th', 'td', 'br', 'hr', 'script', 'style', 'template', 'noscript',
]);

export const HTML_VOID_ELEMENT_SET = new Set([
    'area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta',
    'source', 'track', 'wbr',
]);
