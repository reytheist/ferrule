import { AddAccountModal } from "./features/accounts/AddAccountModal";
import { AgentActivityModal } from "./features/agent/AgentActivityModal";
import { CalendarView } from "./features/calendar/CalendarView";
import { CommandBar } from "./features/command-bar/CommandBar";
import { ComposeModal } from "./features/compose/ComposeModal";
import { EmptyWorkspace } from "./features/onboarding/EmptyWorkspace";
import { OnboardingModal } from "./features/onboarding/OnboardingModal";
import { ReadingPane } from "./features/reading/ReadingPane";
import { SettingsModal } from "./features/settings/SettingsModal";
import { MailboxSidebar } from "./features/sidebar/MailboxSidebar";
import { SpaceSwitcher } from "./features/spaces/SpaceSwitcher";
import { useMailApp } from "./features/shell/useMailApp";
import { ThreadList } from "./features/threads/ThreadList";
import { Topbar } from "./features/topbar/Topbar";
import { TemplatesModal } from "./features/templates/TemplatesModal";
import { FeatureTour } from "./features/tour/FeatureTour";
import { RulesModal } from "./features/workflow/RulesModal";
import { ScreenerModal } from "./features/workflow/ScreenerModal";
import { FerruleGlyph } from "./features/ui/FerruleGlyph";
import { SchedulePopover } from "./features/ui/SchedulePopover";
import { UndoToast } from "./features/ui/UndoToast";

export function App() {
  const app = useMailApp();

  if (app.booting) {
    return (
      <div className="boot-screen">
        <div className="brand-mark">
          <span className="logo-dot">
            <FerruleGlyph />
          </span>
          <span>Ferrule</span>
        </div>
        <span>Booting mail core…</span>
      </div>
    );
  }

  if (app.error) {
    return (
      <div className="boot-screen error">
        <div className="brand-mark">
          <span className="logo-dot">
            <FerruleGlyph />
          </span>
          <span>Ferrule</span>
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
        {app.activeSection === "calendar" ? (
          <CalendarView app={app} />
        ) : app.accounts.length === 0 ? (
          <EmptyWorkspace app={app} />
        ) : (
          <>
            <ThreadList app={app} />
            <ReadingPane app={app} />
          </>
        )}
      </div>

      {app.onboardingOpen && <OnboardingModal app={app} />}
      {app.tourOpen && <FeatureTour app={app} />}
      {app.commandOpen && <CommandBar app={app} />}
      {app.composeOpen && <ComposeModal app={app} />}
      {app.addAccountOpen && <AddAccountModal app={app} />}
      {app.templatesOpen && <TemplatesModal app={app} />}
      {app.settingsOpen && <SettingsModal app={app} />}
      {app.screenerOpen && <ScreenerModal app={app} />}
      {app.rulesOpen && <RulesModal app={app} />}
      {app.agentModalOpen && <AgentActivityModal app={app} />}
      <SchedulePopover app={app} />
      <UndoToast app={app} />
    </div>
  );
}
