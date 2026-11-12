Feature: Open Anything
  As someone who knows the name of what they want
  I want one box that opens any thread, panel, project or page
  So I can get there without hunting through the sidebar and the rails

  # GPUI-only: the Swift app is frozen and hasn't got the box (Mike picked
  # this design, "A, the palette", in thread 5d33618f), so every scenario
  # here is @swift-pending.

  Background:
    Given the app is connected to a runner with the demo threads

  @nav.openanything @swift-pending
  Scenario: ⌘K puts the box up with the pages and their keys, and Esc puts it away
    When I press "k" with "cmd"
    Then the Open Anything box is up
    And the box lists "Inbox | ⌘1" under "Go to"
    And the box lists "Settings | ⌘," under "Go to"
    And the box's pick is "Inbox"
    When I press Escape
    Then the Open Anything box is down

  @nav.openanything @swift-pending
  Scenario: What was opened lately is under Recent
    When I open the thread "Rename the Settings tabs"
    And I press "k" with "cmd"
    Then the box lists "Rename the Settings tabs | thread" under "Recent"

  @nav.openanything @swift-pending
  Scenario: Typing finds a thread by name, archived ones too, and Return opens it
    When I press "k" with "cmd"
    And I type "cloudflare tunnel" in the Open Anything box
    Then the box's first thread is "Set up the Cloudflare tunnel for the preview server", archived
    When I press Return in the Open Anything box
    Then the open thread is "Set up the Cloudflare tunnel for the preview server"
    And the Open Anything box is down

  @nav.openanything @swift-pending
  Scenario: A panel opens where it's from, with its button lit
    When I press "k" with "cmd"
    And I type "simulator" in the Open Anything box
    Then the box's pick is "Simulator"
    When I press Return in the Open Anything box
    Then the "Simulator" button in the right rail is lit

  @nav.openanything @swift-pending
  Scenario: Settings' sections are there to type
    When I press "k" with "cmd"
    And I type "keyboard" in the Open Anything box
    Then the box lists "Settings › Keyboard | ⌘," under "App"
    When I press Return in the Open Anything box
    Then Settings shows "keyboard"

  @nav.openanything @swift-pending
  Scenario: The arrows move the pick, and ⌘Return opens a thread in a panel of its own
    When I press "k" with "cmd"
    And I press the down arrow
    Then the box's pick is "Scheduled"
    When I type "dark mode" in the Open Anything box
    Then the box's pick is "Add dark mode to the dashboard"
    When I press ⌘Return in the Open Anything box
    Then "Add dark mode to the dashboard" is in a panel of its own, showing its transcript
