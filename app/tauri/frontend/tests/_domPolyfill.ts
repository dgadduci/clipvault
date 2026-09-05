/**
 * Minimal DOM polyfill used by the drag-and-drop integration tests.
 *
 * Node.js ships `EventTarget` natively but not `Document`,
 * `HTMLElement` or `DataTransfer`. Svelte 5's `mount()` requires a
 * real DOM target so the integration tests that dispatch the
 * `dragstart → dragenter → dragover → drop → card-drop` chain
 * cannot run without a polyfill. The helpers below implement just
 * enough surface for the Svelte runtime + the drag handlers we
 * exercise.
 *
 * The polyfill deliberately does NOT extend Node's `EventTarget`
 * because the native implementation enforces `event instanceof
 * Event` and rejects custom subclasses. Every element exposes a
 * tiny `addEventListener` / `removeEventListener` /
 * `dispatchEvent` triple the test can drive without touching the
 * real DOM.
 */

interface DomEventListener {
  handleEvent(event: DomEvent): void;
}

interface DomNodeLike {
  parentNode: DomElementLike | null;
  listeners: Map<string, Set<DomEventListener>>;
}

interface DomElementLike extends DomNodeLike {
  classList: DomClassListLike;
}

interface DomClassListLike {
  add(...tokens: string[]): void;
  remove(...tokens: string[]): void;
  contains(token: string): boolean;
}

class DomEvent {
  readonly AT_TARGET = 2 as const;
  readonly BUBBLING_PHASE = 3 as const;
  readonly CAPTURING_PHASE = 1 as const;
  readonly NONE = 0 as const;
  bubbles = false;
  cancelBubble = false;
  cancelable = false;
  composed = false;
  currentTarget: DomNodeLike | null = null;
  defaultPrevented = false;
  eventPhase = 0;
  isTrusted = false;
  returnValue = true;
  srcElement: DomNodeLike | null = null;
  target: DomNodeLike | null = null;
  timeStamp = Date.now();
  type: string;
  constructor(type: string, init: { bubbles?: boolean; cancelable?: boolean } = {}) {
    this.type = type;
    if (init.bubbles) this.bubbles = true;
    if (init.cancelable) this.cancelable = true;
  }
  composedPath(): DomNodeLike[] {
    const path: DomNodeLike[] = [];
    let current: DomNodeLike | null = this.target;
    while (current) {
      path.push(current);
      current = current.parentNode;
    }
    return path;
  }
  initEvent(): void {
    /* no-op */
  }
  preventDefault(): void {
    this.defaultPrevented = true;
    this.returnValue = false;
  }
  stopImmediatePropagation(): void {
    this.cancelBubble = true;
  }
  stopPropagation(): void {
    this.cancelBubble = true;
  }
}

class DomDataTransfer {
  private readonly data = new Map<string, string>();
  effectAllowed: "none" | "copy" | "link" | "move" | "all" = "all";
  dropEffect: "none" | "copy" | "link" | "move" = "none";
  readonly typesShim: string[];
  constructor(initial: { types?: string[]; data?: Record<string, string> } = {}) {
    this.typesShim = initial.types ?? [];
    if (initial.data) {
      for (const [k, v] of Object.entries(initial.data)) {
        this.data.set(k, v);
      }
    }
    if (initial.types) {
      for (const type of initial.types) {
        if (!this.data.has(type)) {
          this.data.set(type, "");
        }
      }
    }
  }
  get types(): ReadonlyArray<string> {
    return Array.from(this.data.keys());
  }
  setData(type: string, value: string): void {
    this.data.set(type, String(value));
  }
  getData(type: string): string {
    return this.data.get(type) ?? "";
  }
  clearData(type?: string): void {
    if (type === undefined) {
      this.data.clear();
    } else {
      this.data.delete(type);
    }
  }
}

class DragEventImpl extends DomEvent {
  dataTransfer: DomDataTransfer;
  relatedTarget: DomNodeLike | null = null;
  constructor(
    type: string,
    init: {
      bubbles?: boolean;
      cancelable?: boolean;
      dataTransfer?: DomDataTransfer;
      relatedTarget?: DomNodeLike | null;
    } = {},
  ) {
    super(type, init);
    this.dataTransfer =
      init.dataTransfer ?? new DomDataTransfer({ types: [] });
    this.relatedTarget = init.relatedTarget ?? null;
  }
}

class MouseEventImpl extends DomEvent {
  clientX = 0;
  clientY = 0;
  button = 0;
  buttons = 0;
  relatedTarget: DomNodeLike | null = null;
  constructor(
    type: string,
    init: {
      bubbles?: boolean;
      cancelable?: boolean;
      clientX?: number;
      clientY?: number;
      button?: number;
      relatedTarget?: DomNodeLike | null;
    } = {},
  ) {
    super(type, init);
    this.clientX = init.clientX ?? 0;
    this.clientY = init.clientY ?? 0;
    this.button = init.button ?? 0;
    this.relatedTarget = init.relatedTarget ?? null;
  }
}

class PointerEventImpl extends MouseEventImpl {
  pointerId = 1;
  pointerType = "mouse";
  constructor(
    type: string,
    init: {
      bubbles?: boolean;
      cancelable?: boolean;
      clientX?: number;
      clientY?: number;
      button?: number;
      pointerId?: number;
      pointerType?: string;
    } = {},
  ) {
    super(type, init);
    this.pointerId = init.pointerId ?? 1;
    this.pointerType = init.pointerType ?? "mouse";
  }
}

class KeyboardEventImpl extends DomEvent {
  key = "";
  code = "";
  altKey = false;
  ctrlKey = false;
  metaKey = false;
  shiftKey = false;
  constructor(
    type: string,
    init: {
      bubbles?: boolean;
      cancelable?: boolean;
      key?: string;
      code?: string;
      altKey?: boolean;
      ctrlKey?: boolean;
      metaKey?: boolean;
      shiftKey?: boolean;
    } = {},
  ) {
    super(type, init);
    this.key = init.key ?? "";
    this.code = init.code ?? "";
    this.altKey = init.altKey ?? false;
    this.ctrlKey = init.ctrlKey ?? false;
    this.metaKey = init.metaKey ?? false;
    this.shiftKey = init.shiftKey ?? false;
  }
}

class CustomEventImpl<T = unknown> extends DomEvent {
  detail: T;
  constructor(
    type: string,
    init: { detail?: T; bubbles?: boolean; cancelable?: boolean } = {},
  ) {
    super(type, init);
    this.detail = (init.detail ?? null) as T;
  }
}

class DomClassList {
  private owner: DomElement;
  private tokens = new Set<string>();
  constructor(owner: DomElement) {
    this.owner = owner;
  }
  add(...tokens: string[]): void {
    for (const token of tokens) {
      this.tokens.add(token);
    }
    this.syncOwner();
  }
  remove(...tokens: string[]): void {
    for (const token of tokens) {
      this.tokens.delete(token);
    }
    this.syncOwner();
  }
  toggle(token: string, force?: boolean): boolean {
    if (force === true) {
      this.tokens.add(token);
      this.syncOwner();
      return true;
    }
    if (force === false) {
      this.tokens.delete(token);
      this.syncOwner();
      return false;
    }
    if (this.tokens.has(token)) {
      this.tokens.delete(token);
      this.syncOwner();
      return false;
    }
    this.tokens.add(token);
    this.syncOwner();
    return true;
  }
  contains(token: string): boolean {
    return this.tokens.has(token);
  }
  values(): IterableIterator<string> {
    return this.tokens.values();
  }
  replaceAll(tokens: string[]): void {
    this.tokens.clear();
    for (const t of tokens) this.tokens.add(t);
    this.syncOwner();
  }
  private syncOwner(): void {
    this.owner.setAttribute("class", Array.from(this.tokens).join(" "));
  }
}

function matchesSelector(element: DomElement, selector: string): boolean {
  const trimmed = selector.trim();
  if (!trimmed) return false;
  if (trimmed.startsWith("[")) {
    const m = /^\[([a-zA-Z0-9_-]+)(?:([~|^$*]?=)"([^"]*)")?\]$/.exec(trimmed);
    if (!m) return false;
    const [, name, op, value] = m;
    const attr = element.getAttribute(name);
    if (value === undefined) {
      return attr !== null;
    }
    if (!op) return attr === value;
    switch (op) {
      case "=":
        return attr === value;
      case "^=":
        return !!attr && attr.startsWith(value);
      case "$=":
        return !!attr && attr.endsWith(value);
      case "*=":
        return !!attr && attr.includes(value);
      case "|=":
        return !!attr && (attr === value || attr.startsWith(`${value}-`));
      case "~=":
        return !!attr && attr.split(/\s+/).includes(value);
      default:
        return false;
    }
  }
  if (trimmed.startsWith("#")) {
    return element.getAttribute("id") === trimmed.slice(1);
  }
  if (trimmed.startsWith(".")) {
    return element.classList.contains(trimmed.slice(1));
  }
  return element.localName === trimmed.toLowerCase();
}

function querySelectorInternal(
  root: DomElement,
  selector: string,
  all: boolean,
): DomElement[] | DomElement | null {
  const result: DomElement[] = [];
  const visit = (node: DomElement): void => {
    for (const child of node.children) {
      if (matchesSelector(child, selector)) {
        result.push(child);
        if (!all) return;
      }
      visit(child);
      if (!all && result.length > 0) return;
    }
  };
  visit(root);
  return all ? result : result[0] ?? null;
}

function dispatchDomEvent(
  target: DomNodeLike,
  event: DomEvent,
): boolean {
  event.target = target;
  event.currentTarget = target;
  // Phase 1: target.
  const targetHandlers = target.listeners.get(event.type);
  if (targetHandlers) {
    for (const handler of Array.from(targetHandlers)) {
      handler.handleEvent(event);
      if (event.cancelBubble) break;
    }
  }
  // Phase 2: bubble.
  if (event.bubbles && !event.cancelBubble) {
    let current: DomNodeLike | null = target.parentNode;
    while (current) {
      event.currentTarget = current;
      const handlers = current.listeners.get(event.type);
      if (handlers) {
        for (const handler of Array.from(handlers)) {
          handler.handleEvent(event);
          if (event.cancelBubble) break;
        }
      }
      if (event.cancelBubble) break;
      current = current.parentNode;
    }
  }
  // `dispatchEvent` returns `false` ONLY when the event is
  // cancelable AND `preventDefault` was called. The drag handlers
  // call `preventDefault` to opt the row into `drop`; the dispatch
  // itself stays cancellable so the caller can still observe the
  // dispatch happened.
  return !(event.cancelable && event.defaultPrevented);
}

class DomElement implements DomElementLike {
  readonly nodeType = 1;
  readonly ELEMENT_NODE = 1;
  nodeName: string;
  localName: string;
  parentNode: DomElement | null = null;
  childNodes: DomElement[] = [];
  firstChild: DomElement | null = null;
  lastChild: DomElement | null = null;
  attributes = new Map<string, string>();
  classList: DomClassList;
  style: Record<string, string> = {};
  private _innerHTML = "";
  textContent = "";
  ownerDocument: DocumentImpl;
  listeners = new Map<string, Set<DomEventListener>>();
  constructor(tagName: string, ownerDocument: DocumentImpl) {
    this.nodeName = tagName.toUpperCase();
    this.localName = tagName.toLowerCase();
    this.ownerDocument = ownerDocument;
    this.classList = new DomClassList(this);
  }
  get className(): string {
    return Array.from(this.classList.values()).join(" ");
  }
  set className(value: string) {
    const tokens = value.split(/\s+/).filter(Boolean);
    this.classList.replaceAll(tokens);
  }
  get id(): string {
    return this.getAttribute("id") ?? "";
  }
  set id(value: string) {
    this.setAttribute("id", value);
  }
  get dataset(): Record<string, string> {
    const out: Record<string, string> = {};
    for (const [name, value] of this.attributes.entries()) {
      if (name.startsWith("data-")) {
        const key = name.slice(5).replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());
        out[key] = value;
      }
    }
    return out;
  }
  get innerHTML(): string {
    return this._innerHTML;
  }
  set innerHTML(value: string) {
    this._innerHTML = value;
    while (this.childNodes.length > 0) {
      const removed = this.childNodes.pop();
      if (removed) {
        removed.parentNode = null;
      }
    }
    this.firstChild = null;
    this.lastChild = null;
  }
  get children(): DomElement[] {
    return this.childNodes.filter((c): c is DomElement => c.nodeType === 1);
  }
  get nextSibling(): DomElement | null {
    if (!this.parentNode) return null;
    const idx = this.parentNode.childNodes.indexOf(this);
    return idx >= 0 && idx < this.parentNode.childNodes.length - 1
      ? this.parentNode.childNodes[idx + 1]
      : null;
  }
  get previousSibling(): DomElement | null {
    if (!this.parentNode) return null;
    const idx = this.parentNode.childNodes.indexOf(this);
    return idx > 0 ? this.parentNode.childNodes[idx - 1] : null;
  }
  get tagName(): string {
    return this.nodeName;
  }
  appendChild<T extends DomElement>(child: T): T {
    if (child.parentNode) {
      const parent = child.parentNode;
      parent.removeChild(child);
    }
    child.parentNode = this;
    this.childNodes.push(child);
    if (!this.firstChild) this.firstChild = child;
    this.lastChild = child;
    return child;
  }
  insertBefore<T extends DomElement>(child: T, ref: DomElement | null): T {
    if (ref === null) {
      this.appendChild(child);
      return child;
    }
    const idx = this.childNodes.indexOf(ref);
    if (idx < 0) {
      this.appendChild(child);
      return child;
    }
    if (child.parentNode) {
      child.parentNode.removeChild(child);
    }
    child.parentNode = this;
    this.childNodes.splice(idx, 0, child);
    if (idx === 0) this.firstChild = child;
    if (idx === this.childNodes.length - 1) this.lastChild = child;
    return child;
  }
  removeChild<T extends DomElement>(child: T): T {
    const idx = this.childNodes.indexOf(child);
    if (idx < 0) {
      throw new Error("removeChild: child not in parent");
    }
    this.childNodes.splice(idx, 1);
    if (this.childNodes.length === 0) {
      this.firstChild = null;
      this.lastChild = null;
    } else {
      this.firstChild = this.childNodes[0];
      this.lastChild = this.childNodes[this.childNodes.length - 1];
    }
    child.parentNode = null;
    return child;
  }
  setAttribute(name: string, value: string): void {
    this.attributes.set(name, String(value));
  }
  removeAttribute(name: string): void {
    this.attributes.delete(name);
  }
  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }
  hasAttribute(name: string): boolean {
    return this.attributes.has(name);
  }
  contains(other: DomElement | null): boolean {
    if (other === null) return false;
    if (other === this) return true;
    let current: DomElement | null = other;
    while (current) {
      if (current === this) return true;
      current = current.parentNode;
    }
    return false;
  }
  closest(selector: string): DomElement | null {
    let current: DomElement | null = this;
    while (current) {
      if (matchesSelector(current, selector)) {
        return current;
      }
      current = current.parentNode;
    }
    return null;
  }
  querySelector(selector: string): DomElement | null {
    return querySelectorInternal(this, selector, false) as DomElement | null;
  }
  querySelectorAll(selector: string): DomElement[] {
    return querySelectorInternal(this, selector, true) as DomElement[];
  }
  addEventListener(
    type: string,
    listener: ((event: DomEvent) => void) | DomEventListener | null,
    _options?: AddEventListenerOptions | boolean,
  ): void {
    if (!listener) return;
    const callable: DomEventListener =
      typeof listener === "function"
        ? { handleEvent: (event) => listener(event) }
        : listener;
    let bucket = this.listeners.get(type);
    if (!bucket) {
      bucket = new Set();
      this.listeners.set(type, bucket);
    }
    bucket.add(callable);
  }
  removeEventListener(
    type: string,
    listener: ((event: DomEvent) => void) | DomEventListener | null,
    _options?: EventListenerOptions | boolean,
  ): void {
    if (!listener) return;
    const callable: DomEventListener =
      typeof listener === "function"
        ? { handleEvent: (event) => listener(event) }
        : listener;
    const bucket = this.listeners.get(type);
    if (bucket) bucket.delete(callable);
  }
  dispatchEvent(event: DomEvent): boolean {
    return dispatchDomEvent(this, event);
  }
  click(): void {
    const ev = new MouseEventImpl("click", { bubbles: true, cancelable: true });
    this.dispatchEvent(ev);
  }
  focus(): void {
    const ev = new DomEvent("focus", { bubbles: false });
    this.dispatchEvent(ev);
  }
  blur(): void {
    const ev = new DomEvent("blur", { bubbles: false });
    this.dispatchEvent(ev);
  }
}

class DocumentImpl {
  body: DomElement;
  head: DomElement;
  documentElement: DomElement;
  private idCounter = 0;
  hitTestElement: DomElement | null = null;
  listeners = new Map<string, Set<DomEventListener>>();
  constructor() {
    this.documentElement = new DomElement("html", this);
    this.head = new DomElement("head", this);
    this.body = new DomElement("body", this);
    this.documentElement.appendChild(this.head);
    this.documentElement.appendChild(this.body);
    // The real DOM bubbles element events through document. The tiny
    // polyfill keeps the same path so the singleton pointer controller can
    // be tested without mounting a full browser DOM.
    (this.documentElement as unknown as { parentNode: DocumentImpl }).parentNode =
      this;
  }
  addEventListener(
    type: string,
    listener: ((event: DomEvent) => void) | DomEventListener | null,
    _options?: AddEventListenerOptions | boolean,
  ): void {
    if (!listener) return;
    const callable: DomEventListener =
      typeof listener === "function"
        ? { handleEvent: (event) => listener(event) }
        : listener;
    let bucket = this.listeners.get(type);
    if (!bucket) {
      bucket = new Set();
      this.listeners.set(type, bucket);
    }
    bucket.add(callable);
  }
  removeEventListener(
    type: string,
    listener: ((event: DomEvent) => void) | DomEventListener | null,
    _options?: EventListenerOptions | boolean,
  ): void {
    if (!listener) return;
    const callable: DomEventListener =
      typeof listener === "function"
        ? { handleEvent: (event) => listener(event) }
        : listener;
    const bucket = this.listeners.get(type);
    if (bucket) bucket.delete(callable);
  }
  dispatchEvent(event: DomEvent): boolean {
    return dispatchDomEvent(this, event);
  }
  createElement(tagName: string): DomElement {
    return new DomElement(tagName, this);
  }
  elementFromPoint(_clientX: number, _clientY: number): DomElement | null {
    return this.hitTestElement;
  }
  createTextNode(data: string): DomElement {
    const node = new DomElement("#text", this);
    node.textContent = data;
    return node;
  }
  createElementNS(_ns: string, tagName: string): DomElement {
    return this.createElement(tagName);
  }
  getElementById(id: string): DomElement | null {
    const visit = (node: DomElement): DomElement | null => {
      if (node.getAttribute("id") === id) return node;
      for (const child of node.children) {
        const found = visit(child);
        if (found) return found;
      }
      return null;
    };
    return visit(this.documentElement);
  }
  querySelector(selector: string): DomElement | null {
    const result = querySelectorInternal(this.documentElement, selector, false);
    return result;
  }
  querySelectorAll(selector: string): DomElement[] {
    const result = querySelectorInternal(this.documentElement, selector, true);
    return result as DomElement[];
  }
  nextUniqueId(prefix: string): string {
    this.idCounter += 1;
    return `${prefix}-${this.idCounter}`;
  }
}

function installDomPolyfill(): {
  document: DocumentImpl;
  window: { document: DocumentImpl };
  restore: () => void;
} {
  const document = new DocumentImpl();
  const windowObj = { document };
  const globalScope = globalThis as unknown as Record<string, unknown>;
  const previous = {
    document: globalScope.document,
    window: globalScope.window,
    HTMLElement: globalScope.HTMLElement,
    HTMLInputElement: globalScope.HTMLInputElement,
    HTMLButtonElement: globalScope.HTMLButtonElement,
    HTMLLIElement: globalScope.HTMLLIElement,
    HTMLDivElement: globalScope.HTMLDivElement,
    HTMLParagraphElement: globalScope.HTMLParagraphElement,
    HTMLSpanElement: globalScope.HTMLSpanElement,
    HTMLUListElement: globalScope.HTMLUListElement,
    HTMLFormElement: globalScope.HTMLFormElement,
    HTMLAnchorElement: globalScope.HTMLAnchorElement,
    HTMLImageElement: globalScope.HTMLImageElement,
    HTMLHeadingElement: globalScope.HTMLHeadingElement,
    Event: globalScope.Event,
    DragEvent: globalScope.DragEvent,
    MouseEvent: globalScope.MouseEvent,
    PointerEvent: globalScope.PointerEvent,
    KeyboardEvent: globalScope.KeyboardEvent,
    CustomEvent: globalScope.CustomEvent,
    Element: globalScope.Element,
    Node: globalScope.Node,
    NodeList: globalScope.NodeList,
  };
  globalScope.document = document;
  globalScope.window = windowObj;
  globalScope.HTMLElement = DomElement;
  globalScope.HTMLInputElement = DomElement;
  globalScope.HTMLButtonElement = DomElement;
  globalScope.HTMLLIElement = DomElement;
  globalScope.HTMLDivElement = DomElement;
  globalScope.HTMLParagraphElement = DomElement;
  globalScope.HTMLSpanElement = DomElement;
  globalScope.HTMLUListElement = DomElement;
  globalScope.HTMLFormElement = DomElement;
  globalScope.HTMLAnchorElement = DomElement;
  globalScope.HTMLImageElement = DomElement;
  globalScope.HTMLHeadingElement = DomElement;
  globalScope.Event = DomEvent;
  globalScope.DragEvent = DragEventImpl;
  globalScope.MouseEvent = MouseEventImpl;
  globalScope.PointerEvent = PointerEventImpl;
  globalScope.KeyboardEvent = KeyboardEventImpl;
  globalScope.CustomEvent = CustomEventImpl;
  globalScope.Element = DomElement;
  globalScope.Node = DomElement;
  globalScope.NodeList = Array;
  const restore = (): void => {
    for (const [key, value] of Object.entries(previous)) {
      if (value === undefined) {
        delete (globalScope as Record<string, unknown>)[key];
      } else {
        (globalScope as Record<string, unknown>)[key] = value;
      }
    }
  };
  return { document, window: windowObj, restore };
}

export {
  installDomPolyfill,
  DomElement,
  DocumentImpl,
  DomDataTransfer,
  DragEventImpl,
  MouseEventImpl,
  PointerEventImpl,
  KeyboardEventImpl,
  CustomEventImpl,
};
