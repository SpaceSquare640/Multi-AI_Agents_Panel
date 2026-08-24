import { useState } from "react";
import { useTranslation } from "react-i18next";
import GameAgent from "./GameAgent";
import SemanticSearch from "./SemanticSearch";
import "./MachineLearning.css";

/** Machine Learning is the parent tab; Semantic Search and Game Agent are
 *  its two branches, switched with an in-page sub-nav rather than being
 *  separate top-level tabs — per explicit user correction: Game Agent is
 *  a branch of Machine Learning, not its own category. */
type Branch = "semantic-search" | "game-agent";

export default function MachineLearning() {
  const { t } = useTranslation();
  const [branch, setBranch] = useState<Branch>("semantic-search");

  return (
    <div className="ml-page">
      <h1>{t("ml.title")}</h1>
      <div className="ml-branch-nav">
        <button className={branch === "semantic-search" ? "active" : ""} onClick={() => setBranch("semantic-search")}>
          {t("semanticSearch.title")}
        </button>
        <button className={branch === "game-agent" ? "active" : ""} onClick={() => setBranch("game-agent")}>
          {t("gameAgentPage.title")}
        </button>
      </div>
      <div hidden={branch !== "semantic-search"}>
        <SemanticSearch />
      </div>
      <div hidden={branch !== "game-agent"}>
        <GameAgent />
      </div>
    </div>
  );
}
