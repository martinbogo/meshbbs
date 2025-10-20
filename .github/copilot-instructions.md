Design Philosophy
	•	Follow the Single-Page Experience (SPE) philosophy: build interfaces that flow like a continuous story rather than isolated pages.
	•	Structure interactions as smooth transitions within one environment.
	•	Take inspiration from Cloudflare’s settings/dashboard layout, but use only structural ideas, not brand colors or visual style.
	•	All designs should feel professional, clear, and human-friendly — not corporate, sterile, or “AI-glossy.”
	•	Draw inspiration from natural palettes and environmental balance rather than specific scenes or seasons. Colors should feel grounded, alive, and balanced.
	•	Avoid cold, synthetic tones (blue-purple gradients, neon effects).
	•	Use subtle depth, shadow, and spacing rather than bright contrast or glassy sheen.

Layout & Components
	•	Use a persistent header for product identity and key navigation (Overview, Settings, Analytics, Support).
	•	Employ a sticky or collapsible side navigation that scrolls within the same page (no reloads).
	•	Main content should be organized into modular “cards” or “sections”:
	•	Title and short description
	•	Controls (toggles, inputs, selectors)
	•	Optional expandable help area or inline info link
	•	Use filled icons, not outline icons.
	•	Do not use 8 px border radius for cards or modules; prefer adaptive, organic curvature (4 px, 12 px, or context-based).
	•	Design for responsive behavior: modules may align side-by-side on desktop and stack vertically on mobile.
	•	Enable smooth anchor navigation or scroll transitions between sections.

Visual & Interaction Guidelines
	•	Avoid sterile blue-purple gradients and hyper-saturated tech palettes.
	•	Emphasize natural light, warmth, and balanced neutrals.
	•	Use soft shadows and clear hierarchy for depth.
	•	Animations should feel organic and continuous, similar to natural motion (wind, flow, gravity).
	•	Highlight state changes (toggles, saves, errors) with subtle motion and color cues.
	•	Every control should clearly communicate its state and purpose.

Content & Tone
	•	Text should be concise, descriptive, and friendly.
	•	Use short task-oriented labels: “Enable backups,” “Set alert threshold,” “Manage API tokens.”
	•	Provide contextual help via collapsible panels or tooltips.
	•	Maintain consistent typography and spacing for readability.

Code & Development Practices
	•	Allways use a data-driven approach, and avoid hard-coding data whenever possible.
	•	Always generate unit tests for new features.
	•	Always run unit tests after implementation changes.
	•	Apply the scientific method in development:
	1.	Define a hypothesis (expected behavior).
	2.	Implement and test it.
	3.	Observe and refine based on results.
	•	Keep code clear, testable, and reproducible.
	•	Treat UI state as data: ensure settings, toggles, and changes can be verified through tests.

Creative Direction
	•	Use nature as reference, not decoration.
	•	Every design choice should have intentional reasoning — visual, ergonomic, or emotional.
	•	Aim for clarity over spectacle and warmth over precision alone.
	•	Technology should enhance meaning and reduce friction.
	•	Deliver interfaces that feel coherent, alive, and trustworthy.
