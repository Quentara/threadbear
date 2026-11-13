Feature: The Updates panel
  As someone whose runner and agent CLIs keep themselves up to date
  I want one place that says what's on its way and what's waiting for me
  So I can see an update coming, and act on the few that need me

  Background:
    Given the app is connected to a runner with the demo threads

  @updates.panel
  Scenario: It says how the runner's update is going and what each CLI has
    When the runner starts building main at "abc1234" for "Fix the thing"
    And I open the Updates panel
    Then the Updates panel lists "Building the runner from main at abc1234: Fix the thing."
    And the Updates panel lists "Claude Code 2.1.311, 2.1.312 available (automatic) [Update]"
    And the Updates panel lists "Codex 0.165.0 is held back because the app-server protocol changed (automatic) [Update anyway] [Go back to 0.163.0]"
    And the Updates panel lists "  Missing: thread/backgroundTerminals/list"

  @updates.panel
  Scenario: Update installs a CLI's new version, saying so while it runs
    When I open the Updates panel
    And I press "Install the new version now" in the Updates panel
    Then the Updates panel lists "Claude Code 2.1.311, 2.1.312 available (automatic): Updating Claude Code to 2.1.312"
    And the Updates panel lists "Claude Code 2.1.312, up to date (automatic)"

  @updates.panel
  Scenario: Mike's release says what's new, and Bring Them In starts a thread for it
    When Mike has a release of 10 Nov with 3 new changes
    And I open the Updates panel
    Then the Updates panel lists "Mike's release of 10 Nov has 3 new changes."
    And the Updates panel lists "  Runners in the app"
    When I press "Start a thread that merges Mike's release into your main" in the Updates panel
    Then a thread called "Bring in Mike's release of 10 Nov" opens

  # Someone's fork with changes of their own to the old Swift app (protocol
  # "Your changes to the old app"). The Swift app has nothing to rebuild.
  @updates.swiftchanges @swift-pending
  Scenario: My own changes to the old app are listed, and Port this starts a thread for one
    When my copy has 2 changes of its own to the old app
    And I open the Updates panel
    Then the Updates panel lists "You changed the old app in 2 places that aren't in this one yet. Port each to rebuild it here, or dismiss it."
    And the Updates panel lists "  Orange dock badge (2 Nov · abc1234 · Badge.swift)"
    And the Updates panel lists "  Bigger sidebar font (20 Oct · 0123456 · Sidebar.swift)"
    # A press goes to the last button with that tip: the second change's.
    When I press "Leave this change out of the new app" in the Updates panel
    Then the Updates panel lists "You changed the old app in 1 place that isn't in this one yet. Port it to rebuild it here, or dismiss it."
    When I press "Start a thread that rebuilds this change in the new app" in the Updates panel
    Then a thread called 'Rebuild "Orange dock badge" in the new app' opens
    And the Updates panel lists '  > Being rebuilt in Rebuild "Orange dock badge" in the new app'

  # The app's own update is made up (`updates` with `preview`): a scripted
  # app never follows main, as only the installed one does.
  @system.selfupdate
  Scenario: The app's next version waiting says so in the Updates panel
    When the app's next version, def5678, is ready
    And I open the Updates panel
    Then the Updates panel lists "The app's next version (def5678) is ready. It switches in when you're not using the app, or now."

  # The Swift app's menu bar didn't show the made-up update's item within 5 s
  # of a script making it up (its SwiftUI commands, read by `menus`), so this
  # is only checked on the GPUI app for now.
  @system.selfupdate @swift-pending
  Scenario: Restart to Update is in the app menu once the next version is ready
    When the app's next version, def5678, is ready
    Then the app menu has "Restart to Update to def5678"

  # The Swift app builds where it is, so it never waits for a Mac.
  @system.selfupdate @swift-pending
  Scenario: A new version with no Mac to build it on says it's waiting for one
    When the app's next version is waiting for a Mac to build it on
    And I open the Updates panel
    Then the Updates panel lists "The app has a new version on main, waiting for a Mac to build it on."

  # Check hosts only run tests: what an update's build host sends back runs
  # here, so it's only one named in updateBuildHosts (Daybreak 108). With
  # check hosts and none of those it builds nowhere and asks for one.
  @system.selfupdate @swift-pending
  Scenario: With check hosts but no update build host it asks for one
    When no Mac is trusted to build the app's next version
    And I open the Updates panel
    Then the Updates panel lists "The app isn't following main: no Mac is trusted to build it. Check hosts only run tests, so add one you trust with what this Mac runs to updateBuildHosts in runner.json."

  # Going in by itself: straight away with another app in front, and in front
  # once it's had five seconds with no input (core's should_auto_restart, with
  # its tests), but never over words typed or a menu that's open. The GPUI app
  # says what its once-a-second look sees (`updates` with `restartCheck`), as
  # a scripted app never restarts. The Swift app restarts in front only when
  # its handover would go unseen, and has no such check.
  @system.selfupdate @swift-pending
  Scenario: The next version waits to go in while words are typed or a menu is open
    When the app's next version, def5678, is ready
    And I open the thread "Morning brief"
    Then a restart onto it waits for nothing
    When I type "half a thought" in the composer
    Then a restart onto it waits for the words typed
    When I clear the composer
    Then a restart onto it waits for nothing
    When I open the Hibernate menu for "Morning brief"
    Then a restart onto it waits for the open menu

  # Codex's review of the restart, 12 Nov: a card's own answer, a shell
  # running something or typed in, and where the app comes back to.
  @system.selfupdate @swift-pending
  Scenario: The next version waits while my own answer to a card isn't sent
    When the app's next version, def5678, is ready
    And I open the thread "Ship the beta to TestFlight"
    Then a restart onto it waits for nothing
    When I write "not before Friday" as my own answer to ask 1
    Then a restart onto it waits for the words typed

  # Windows has no foreground-process check for ConPTY yet. Every live shell
  # holds a restart, as it could be running a command the app cannot see.
  @system.selfupdate @swift-pending @windows-pending
  Scenario: The next version waits while a shell is typed in or running something
    Given the thread "Rename the Settings tabs" works in a folder of its own
    When the app's next version, def5678, is ready
    And I open the thread "Rename the Settings tabs"
    And I press ⌘J
    Then a restart onto it waits for nothing
    When I type "echo half" in the Terminal without pressing Return
    Then a restart onto it waits for the Terminal
    When I press Return in the Terminal
    Then a restart onto it waits for nothing
    When I type "sleep 30" and press Return in the Terminal
    Then a restart onto it waits for the Terminal

  @system.selfupdate @swift-pending
  Scenario: Back from restarting onto the next version, the list and the thread are as they were
    When the app comes back from restarting onto its next version on the Scheduled list with "Morning brief" open
    Then the sidebar shows the Scheduled list with "Morning brief" open

  # The quick way to take the app's update: a popover by the icon.
  @updates.popover
  Scenario: Right-clicking Updates offers Restart to Update once the next version is ready
    When the app's next version, def5678, is ready
    And I right-click the Updates icon
    Then the update popover says "A new version of the app is ready."

  @updates.popover
  Scenario: The update popover says when the app is up to date
    When I right-click the Updates icon
    Then the update popover says "The app is up to date."

  @updates.panel @swift-pending
  Scenario: An old web runtime says so in Updates
    When the web runtime needs a security update
    And I open the Updates panel
    Then the Updates panel lists "Web runtime: CEF is over two weeks behind stable. Chromium's sandbox is off."
