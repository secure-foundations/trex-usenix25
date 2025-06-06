//Export type information from Listings view to a single machine readable file
//@author Jay Bosamiya
//@category Exporter
//@keybinding
//@menupath
//@toolbar

import java.util.Map;

import ghidra.app.plugin.core.analysis.AutoAnalysisManager;

public class TypesExporter extends TypesDump {
	void dwarf_only() throws Exception {
		// Make sure we only run DWARF analysis, and otherwise run no other auto-analysis
		if (isHeadlessAnalysisEnabled()) {
			throw new Exception("Expected `-noanalysis`");
		}
		resetAllAnalysisOptions(currentProgram);
		for (Map.Entry<String, String> v: this.getCurrentAnalysisOptionsAndValues(currentProgram).entrySet()) {
			if (!v.getKey().contains(".")) {
				if (v.getKey().contentEquals("DWARF")) {
					println("Keeping " + v.getKey() + " enabled");
				} else {
					this.setAnalysisOption(currentProgram, v.getKey(), "false");
				}
			}
		}
		this.enableHeadlessAnalysis(true);
		AutoAnalysisManager mgr = AutoAnalysisManager.getAnalysisManager(currentProgram);
		mgr.startAnalysis(monitor);
		this.analyzeAll(currentProgram);
	}

	@Override
	protected void run() throws Exception {
		this.dwarf_only();
		this.dump_types("types-exported", false, null);
	}
}
