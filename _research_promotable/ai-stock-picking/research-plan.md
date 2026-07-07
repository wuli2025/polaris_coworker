# Research Plan — AI 选股/量化平台竞品深研

## Research question
Deep-dive into global (mostly US) AI stock-picking & quant platforms: detailed step-by-step user workflow (open tool → screen/scan → AI scoring → ranking → entry/exit/stop → backtest), underlying tech (rules vs ML vs DL vs LLM), AI scoring mechanism, backtesting & risk, data coverage, pricing, track record & criticism.

## Audience
Internal competitive-analysis raw material for an "AI quantitative stock-picking system" product team.

## Output
- Type: Full report (structured, one section per product), **in Chinese**
- Number-rich, concrete, with source URLs
- Stakes: Medium (competitive intel, not investment decisions)
- Freshness: prefer 2024-2026 data

## Products / threads
1. Trade Ideas (Holly AI)
2. Tickeron (AI Robots / pattern recognition)
3. Danelfin (AI Score 1-10)
4. Composer (no-code symphonies)
5. QuantConnect (algo backtesting)
6. Numerai (crowdsourced tournament)
7. TrendSpider (automated TA)
8. Kavout (K Score)
9. LLM/ChatGPT-based stock tools (catch-all)

## Orchestration
Lead + subagents. 9 separable threads → batch into ~5 subagents (group light ones). Each owns crisp thread, writes notes file. Lead synthesizes final Chinese report.

## Done =
Each product: numbered workflow steps, tech stack, scoring mechanism, backtest/risk, data coverage, pricing tiers (USD/mo), track record + criticism, cited URLs.
