/** Default launcher backdrop: a still dark field with a low accent haze. `animated` is accepted for
 *  compatibility with the background settings; the backdrop no longer moves on its own. */
export function LauncherGradientBackground(_props: { animated?: boolean }) {
  return (
    <div className="launcher-gradient" aria-hidden="true">
      <span className="launcher-gradient-sheen" />
      <span className="launcher-gradient-grain" />
    </div>
  );
}
