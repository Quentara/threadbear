Feature: How the window looks
  As someone who has Threadbear open all day
  I want its words in Geist, over a frosted painting or a picture of mine,
  with the panels tinted glass
  So the window is pleasant to look at and the words stay easy to read

  Background:
    Given the app is connected to a runner with the demo threads

  # The Swift app is frozen (AGENTS.md), and its dump has no `look`.
  @foundation.fonts @swift-pending
  Scenario: The words are in Geist and code in Geist Mono
    Then the words are in Geist, or the system's font where it isn't installed
    And code is in Geist Mono, or the system's monospaced font where it isn't installed

  @foundation.backdrop @swift-pending
  Scenario: The painting is behind the window, with the panels tinted over it
    Then the painting is behind the window
    And the panels are see-through over it

  @foundation.backdrop @swift-pending
  Scenario: A new thread's page has the painting behind it too
    When I start a new thread
    Then the new-thread page asks "What should we work on?"
    And the painting is behind the window

  # Choose… opens the system's open panel, which a script can't answer, so
  # the step hands the app the picture as the panel would (`backdrop`).
  @settings.background @swift-pending
  Scenario: A picture of mine can go behind the window, and the painting come back
    When I open Settings
    Then Settings says the background is the painting
    When I choose "support/sample.png" as the background
    Then my picture is behind the window
    And Settings says the background is my picture
    When I press "Go back to the painting Threadbear comes with" in Settings
    Then the painting is behind the window
    And Settings says the background is the painting
