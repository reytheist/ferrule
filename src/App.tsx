import { AddAccountModal } from "./features/accounts/AddAccountModal";
import { CommandBar } from "./features/command-bar/CommandBar";
import { ComposeModal } from "./features/compose/ComposeModal";
import { ReadingPane } from "./features/reading/ReadingPane";
import { MailboxSidebar } from "./features/sidebar/MailboxSidebar";
import { SpaceSwitcher } from "./features/spaces/SpaceSwitcher";
import { useMailApp } from "./features/shell/useMailApp";
import { ThreadList } from "./features/threads/ThreadList";
import { Topbar } from "./features/topbar/Topbar";
import { TemplatesModal } from "./features/templates/TemplatesModal";
import { SchedulePopover } from "./features/ui/SchedulePopover";
import { UndoToast } from "./features/ui/UndoToast";

export function App() {
  const app = useMailApp();

  if (app.booting) {
    return (
      <div className="boot-screen">
        <div className="brand-mark">
          <span className="logo-dot">G</span>
          <span>Galapaghost</span>
        </div>
        <span>Booting mail core…</span>
      </div>
    );
  }

  if (app.error) {
    return (
      <div className="boot-screen error">
        <div className="brand-mark">
          <span className="logo-dot">G</span>
          <span>Galapaghost</span>
        </div>
        <span>Mail core failed to start: {app.error}</span>
      </div>
    );
  }

  return (
    <div className="app-shell" data-layout={app.layout} data-stage={app.stage}>
      <Topbar app={app} />
      <SpaceSwitcher app={app} />
      <div className="workspace">
        <MailboxSidebar app={app} />
        <ThreadList app={app} />
        <ReadingPane app={app} />
      </div>

      {app.commandOpen && <CommandBar app={app} />}
      {app.composeOpen && <ComposeModal app={app} />}
      {app.addAccountOpen && <AddAccountModal app={app} />}
      {app.templatesOpen && <TemplatesModal app={app} />}
      <SchedulePopover app={app} />
      <UndoToast app={app} />
    </div>
  );
}
