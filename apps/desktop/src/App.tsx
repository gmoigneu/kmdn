import { useUi } from "./lib/store";
import { Sidebar } from "./components/Sidebar";
import { SignIn } from "./screens/SignIn";
import { Home } from "./screens/Home";
import { ThreadView } from "./screens/ThreadView";
import { DocumentView } from "./screens/DocumentView";
import { ReviewView } from "./screens/ReviewView";

export default function App() {
  const { kb, view } = useUi();
  if (!kb) return <SignIn />;
  return (
    <div className="flex h-full">
      <Sidebar />
      <main className="flex-1 min-w-0 flex flex-col">
        {view.kind === "home" && <Home />}
        {view.kind === "thread" && <ThreadView key={view.slug} slug={view.slug} initialPath={view.openPath} />}
        {view.kind === "document" && <DocumentView path={view.path} />}
        {view.kind === "review" && <ReviewView number={view.number} />}
      </main>
    </div>
  );
}
