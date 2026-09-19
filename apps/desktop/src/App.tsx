import { useUi } from "./lib/store";
import { Sidebar } from "./components/Sidebar";
import { OpenKb } from "./screens/OpenKb";
import { Home } from "./screens/Home";
import { ThreadView } from "./screens/ThreadView";
import { DocumentView } from "./screens/DocumentView";

export default function App() {
  const { kb, view } = useUi();
  if (!kb) return <OpenKb />;
  return (
    <div className="flex h-full">
      <Sidebar />
      <main className="flex-1 min-w-0 flex flex-col">
        {view.kind === "home" && <Home />}
        {view.kind === "thread" && <ThreadView slug={view.slug} />}
        {view.kind === "document" && <DocumentView path={view.path} />}
        {view.kind === "review" && <div className="p-6 text-fg-muted">Review layout lands in #28.</div>}
      </main>
    </div>
  );
}
