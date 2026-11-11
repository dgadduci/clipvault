/** Stop browser-level zoom gestures on application surfaces. */
export function blockWebviewPageZoom(documentTarget: Document): () => void {
  const onWheel = (event: WheelEvent): void => {
    if (!event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
  };
  const onGesture = (event: Event): void => {
    event.preventDefault();
  };
  const options: AddEventListenerOptions = { capture: true, passive: false };

  documentTarget.addEventListener("wheel", onWheel, options);
  documentTarget.addEventListener("gesturestart", onGesture, options);
  documentTarget.addEventListener("gesturechange", onGesture, options);
  documentTarget.addEventListener("gestureend", onGesture, options);

  return () => {
    documentTarget.removeEventListener("wheel", onWheel, true);
    documentTarget.removeEventListener("gesturestart", onGesture, true);
    documentTarget.removeEventListener("gesturechange", onGesture, true);
    documentTarget.removeEventListener("gestureend", onGesture, true);
  };
}
