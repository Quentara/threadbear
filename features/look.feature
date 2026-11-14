Feature: How the window looks
  As someone who has Threadbear open all day
  I want its words in Geist
  So the window is pleasant to look at and the words stay easy to read

  Background:
    Given the app is connected to a runner with the demo threads

  # The Swift app is frozen (AGENTS.md), and its dump has no `look`.
  @foundation.fonts @swift-pending
  Scenario: The words are in Geist and code in Geist Mono
    Then the words are in Geist, or the system's font where it isn't installed
    And code is in Geist Mono, or the system's monospaced font where it isn't installed
