import { useState } from "react";

const EXAMPLES = [
  "Osimhen sezon sonuna kadar sakatlandı",
  "Fenerbahçe sacks its manager",
  "Galatasaray's first-choice keeper is suspended for five matches",
];

export function ScenarioPrompt({
  onSubmit,
  disabled,
}: {
  onSubmit: (prompt: string) => void;
  disabled: boolean;
}) {
  const [prompt, setPrompt] = useState("");

  return (
    <section className="panel scenario-panel" aria-label="What-if scenario">
      <header className="panel-head">
        <h3>What if…</h3>
      </header>
      <div className="panel-body">
        <textarea
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          placeholder="Describe an injury, suspension, transfer or manager change — the model adjusts club ratings and re-simulates the season."
          rows={3}
          maxLength={2000}
          disabled={disabled}
        />
        <div className="scenario-actions">
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => prompt.trim() && onSubmit(prompt)}
            disabled={disabled || !prompt.trim()}
          >
            {disabled ? "Analyzing…" : "Run scenario"}
          </button>
        </div>
        <div className="chips">
          {EXAMPLES.map((ex) => (
            <button
              key={ex}
              type="button"
              className="chip"
              onClick={() => setPrompt(ex)}
              disabled={disabled}
            >
              {ex}
            </button>
          ))}
        </div>
      </div>
    </section>
  );
}
