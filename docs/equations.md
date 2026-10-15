# Equation input

Updated October 14, 2026. Status: editable linear equations with math shortcuts and contextual editing.

Click **Insert → Equation**. A text box appears immediately with **Type equation here**
selected. Start typing to replace the prompt. If you are already editing a text box, the
equation is inserted at that caret, alongside the existing text.

Entering an equation switches the ribbon to the **Equation** tab. This tab is currently empty.
The active equation has a light gray editing shade; this is not saved or exported. Leaving the
equation restores the previously active ribbon tab.

At the end of the equation, press **Right Arrow** once to continue typing ordinary text in the
same text box. No space is inserted. The surrounding text's formatting is restored, so math
italics do not carry over. **Left Arrow** at that boundary returns to the equation; clicking
inside an existing equation also resumes equation editing. You can still choose another ribbon
tab while editing an equation.

Type a command and press **Space** to convert it. That first Space completes the command;
press Space again if you want a visible space. Mathematical delimiters such as `+` and `=`
also complete symbol commands and remain in the equation.

| Input followed by Space | Result |
|---|---|
| `\alpha`, `\beta`, `\theta`, `\pi` | α, β, θ, π |
| `\Gamma`, `\Delta`, `\Sigma`, `\Omega` | Γ, Δ, Σ, Ω |
| `\times`, `\cdot`, `\pm`, `\div` | ×, ⋅, ±, ÷ |
| `\leq`, `\geq`, `\neq`, `\approx` | ≤, ≥, ≠, ≈ |
| `\infty`, `\partial`, `\nabla`, `\sqrt` | ∞, ∂, ∇, √ |
| `\sum`, `\prod`, `\int` | ∑, ∏, ∫ |
| `\rightarrow`, `\Rightarrow`, `\in`, `\forall` | →, ⇒, ∈, ∀ |
| `x^2`, `a_{12}`, `x^n` | x², a₁₂, xⁿ |

Commands are case sensitive. Unknown commands stay as typed. Ordinary text outside the
equation is not converted. Undo immediately after a conversion restores the typed command.
You can move the caret, select and replace characters, paste a linear expression, and continue
editing after saving and reopening.

This is a subset of LaTeX-style input, not a complete LaTeX typesetter. Scripts use available
Unicode characters; unsupported scripts stay verbatim. `\sqrt` inserts a root symbol without
building a radical over its argument. Stacked fractions, matrices, scalable delimiters and
structural editing of imported OMML equations are not implemented here.

Native `.deckcraft` files retain the equation regions. PPTX exports use standard editable text
plus a DeckCraft extension identifying each equation region; DeckCraft reads that extension on
reopen. PowerPoint displays these as text, rather than native equation objects. Other editors
may discard the extension if they resave the file. Existing imported OMML preservation is
unchanged.

Older DeckCraft builds do not recognize the new native equation run kind. Use the PPTX text
fallback when sharing equations with those builds.

The interaction follows Microsoft's documented [Math AutoCorrect behavior](https://support.microsoft.com/en-au/accessibility/word/quick-start-guide-to-math-autocorrect-commands-and-symbols).
The implementation and UI are original; no Microsoft assets are included.
