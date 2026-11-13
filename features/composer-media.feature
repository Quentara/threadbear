Feature: Pictures from other Mac apps
  Pictures dragged or pasted into a message belong to Threadbear before
  the source app takes back its file or access grant.

  Background:
    Given the app is connected to a runner with the demo threads
    And I open the thread "Morning brief"

  # The native pasteboard driver is Mac GPUI's. Swift is frozen; Windows
  # and Linux use their own clipboard formats and the existing file specs.
  @composer.filedrop @composer.picturechips @swift-pending @windows-pending @linux-pending
  Scenario: A screenshot still attaches after its temporary source disappears
    When I type "look at " in the composer
    And I drop a temporary screenshot and its source disappears
    Then the composer holds 1 picture
    When I press Return
    Then the transcript shows my message "look at [Picture 1]"

  @composer.filedrop @composer.picturechips @swift-pending @windows-pending @linux-pending
  Scenario: A browser image with bytes and a URL attaches as a picture
    When I type "look at " in the composer
    And I drop a browser image without a file
    Then the composer holds 1 picture
    When I press Return
    Then the transcript shows my message "look at [Picture 1]"

  @composer.picturechips @swift-pending @windows-pending @linux-pending
  Scenario: A clipboard with only HEIC attaches as a picture
    When I type "look at " in the composer
    And I paste a HEIC picture from the native clipboard
    Then the composer holds 1 picture
    When I press Return
    Then the transcript shows my message "look at [Picture 1]"

  @composer.picturechips @swift-pending @windows-pending @linux-pending
  Scenario: Photos image data replaces its unreadable library path and keeps other files
    When I type "look at " in the composer
    And I paste a Photos picture beside a normal picture
    Then the composer holds 2 pictures
    When I press Return
    Then the transcript shows my message "look at [Picture 1] [Picture 2]"

  # A script can't grab macOS's floating screenshot thumbnail. The first
  # scenario tests its file lifetime through the AppKit drop callbacks.
  @composer.filedrop @manual
  Scenario: A screenshot dragged from the floating thumbnail attaches
    When I drag a screenshot's floating thumbnail onto the thread
    Then the composer holds 1 picture

  @composer.picturechips @swift-pending @windows-pending @linux-pending
  Scenario: An unreadable temporary file URL uses the image bytes beside it
    When I paste a picture with an unavailable file URL
    Then the composer holds 1 picture

  @composer.picturechips @swift-pending @windows-pending @linux-pending @real-screen
  Scenario: A promised photo and a normal picture both arrive
    When I paste a promised photo beside a normal picture
    Then the composer holds 2 pictures

  @composer.filedrop @swift-pending @windows-pending @linux-pending
  Scenario: A browser picture offered only as a web URL attaches
    When I drop a browser picture offered only as a URL
    Then the composer holds 1 picture

  @composer.picturechips @swift-pending @windows-pending @linux-pending
  Scenario: Cutting a picture that is still loading lets me send my words
    When I drop a browser picture whose response is delayed
    And I cut everything in the composer
    And I type "Send these words now" in the composer
    And I press Return
    Then the transcript shows my message "Send these words now"

  @composer.filedrop @swift-pending @windows-pending @linux-pending
  Scenario: Dropping an ordinary web link never fetches it as a picture
    When I drop an ordinary web link
    Then the link has not been fetched

  @composer.filedrop @swift-pending @windows-pending @linux-pending
  Scenario: A linked picture never fetches its enclosing webpage link
    When I drop a linked picture whose link is a webpage
    Then the link has not been fetched

  @composer.picturechips @swift-pending @windows-pending @linux-pending
  Scenario: All pictures in a promised bundle arrive without moving my words
    When I type "compare " in the composer
    And I paste a bundle promising two pictures beside a normal picture
    And I type "please" in the composer
    Then the composer holds 3 pictures
    And the composer says the lines "compare [Picture 1] [Picture 2] [Picture 3] please"

  @composer.picturechips @swift-pending @windows-pending @linux-pending
  Scenario: An unavailable promised picture doesn't hide the bundle's other pictures
    When I paste a bundle with an unavailable picture and two good pictures beside a normal picture
    Then the composer holds 3 pictures

  @composer.picturechips @card.somethingelse @swift-pending @windows-pending @linux-pending
  Scenario: A HEIC-only paste works in a card's Something else box
    When I open the thread "Ship the beta to TestFlight"
    And I write "Like this one " as my own answer to ask 1
    And I paste a HEIC picture into the focused answer box
    Then ask 1 on the card is answered with 1 picture

  @composer.picturechips @swift-pending @windows-pending @linux-pending
  Scenario: A source withdrawing its promise leaves the app usable and keeps other pictures
    When I paste a withdrawn promise beside a normal picture
    Then the composer holds 1 picture
    When I type "still works" in the composer
    Then the composer says the lines "[Picture 1] still works"
