//Export decompiled output (taking variables as input) to a single machine readable file
//@author Jay Bosamiya
//@category Exporter
//@keybinding
//@menupath
//@toolbar

import java.io.BufferedWriter;
import java.io.File;
import java.io.FileWriter;
import java.util.List;
import java.util.Scanner;

import ghidra.app.decompiler.DecompileResults;
import ghidra.app.decompiler.DecompiledFunction;
import ghidra.app.decompiler.flatapi.FlatDecompilerAPI;
import ghidra.app.plugin.core.analysis.AutoAnalysisManager;
import ghidra.app.util.headless.HeadlessScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressSpace;
import ghidra.program.model.data.Undefined;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Listing;
import ghidra.program.model.listing.LocalVariableImpl;
import ghidra.program.model.listing.ParameterImpl;
import ghidra.program.model.listing.Variable;
import ghidra.program.model.pcode.HighFunction;
import ghidra.program.model.pcode.HighSymbol;
import ghidra.program.model.pcode.LocalSymbolMap;
import ghidra.program.model.symbol.SourceType;
import ghidra.program.model.symbol.Symbol;
import ghidra.util.exception.DuplicateNameException;

public class DecompilationDumpWithVariableInputs extends HeadlessScript {
	// Annoying that Ghidra gives us an iterator rather than an iterable sometimes :/
	public static <T> java.lang.Iterable<T> toIterable(java.util.Iterator<T> iterator) {
		return () -> iterator;
	}

	@Override
	protected void run() throws Exception {
		this.input_variables();
		this.rerun_analysis();

		String output_file_name = currentProgram.getName() + ".decompilation-wvi-exported";
		BufferedWriter output_file = new BufferedWriter(new FileWriter(output_file_name));

		FlatDecompilerAPI decomp = null;
		decomp = new FlatDecompilerAPI(this);
		decomp.initialize();

		Listing listing = currentProgram.getListing();
		for (Function f : listing.getFunctions(true)) {
			if (f.isThunk()) {
				continue;
			}
			DecompileResults decompres = decomp.getDecompiler().decompileFunction(f, 30, monitor);
			DecompiledFunction df = decompres.getDecompiledFunction();
			HighFunction hf = decompres.getHighFunction();
			if (hf == null) {
				println("Time limit reached for " + f.getName() + ". Skipping.");
				continue;
			}
			LocalSymbolMap lsm = hf.getLocalSymbolMap();
			output_file.write(df.getC());
			output_file.write("\n\n/************************\n");
			output_file.write("  varmap:\n");
			for (Variable var : f.getAllVariables()) {
				Symbol sym = var.getSymbol();
				if (sym == null) {
					continue;
				}
				String varname = var.getName() + "@" + f.getName() + "@" + f.getEntryPoint();
				if (varname.contains("\t") || var.getName().contains("@") || f.getName().contains("@")) {
					throw new Exception("ERROR: Contains invalid char in variable name: " + varname);
				}
				HighSymbol direcths = lsm.findLocal(sym.getAddress(), null);
				if (direcths != null) {
					output_file.write("    " + varname + ": " + direcths.getName() + "\n");
				} else {
					for (HighSymbol lsmhs : toIterable(lsm.getSymbols())) {
						if (lsmhs.getStorage().intersects(var.getVariableStorage())) {
							output_file.write("    " + varname + ": " + lsmhs.getName() + "\n");
						}
					}
				}
			}
			output_file.write("***************************/\n\n");
		}

		output_file.write("/**<<EOF>>**/");

		output_file.close();
		println("Done exporting to " + output_file_name);
	}

	void rerun_analysis() throws Exception {
		this.resetAllAnalysisOptions(currentProgram);
		this.enableHeadlessAnalysis(true);
		AutoAnalysisManager.getAnalysisManager(currentProgram).startAnalysis(monitor);
		this.analyzeAll(currentProgram);
	}

	void input_variables() throws Exception {
		String input_file_name = this.getScriptArgs()[0];
		if (!input_file_name.endsWith(".vars")) {
			throw new Exception("Expected .vars file path as argument");
		}

		File input_file = new File(input_file_name);
		if (!input_file.exists()) {
			throw new Exception("Expected " + input_file_name + " but not found.");
		}
		Scanner reader = new Scanner(input_file);
		String line;

		// PROGRAM section
		line = reader.nextLine().stripTrailing();
		if(!line.equals("PROGRAM")) {
			reader.close();
			throw new Exception("Expected PROGRAM, got " + line);
		}
		reader.nextLine(); // name
		reader.nextLine(); // stack pointer
		reader.nextLine(); // blank line

		// VARIABLES section
		line = reader.nextLine().stripTrailing();
		if(!line.equals("VARIABLES")) {
			reader.close();
			throw new Exception("Expected VARIABLES, got " + line);
		}
		Listing listing = currentProgram.getListing();
		line = null;
		if(reader.hasNextLine()) {
			line = reader.nextLine().stripTrailing();
		}
		while(line != null) {
			if (line.isBlank()) {
				line = null;
				if(reader.hasNextLine()) {
					line = reader.nextLine().stripTrailing();
				}
				continue;
			}
			if (!(line.startsWith("\t") && !line.startsWith("\t\t"))) {
				reader.close();
				throw new Exception("Parse error. Got line " + line);
			}
			String varname = line.strip().split("@")[0];
			String funcname = line.strip().split("@")[1];
			int funcaddr = Integer.parseInt(line.strip().split("@")[2], 16);
			Function f = null;
			for (Function it : listing.getFunctions(true)) {
				if (it.getEntryPoint().getOffset() == funcaddr) {
					f = it;
				}
			}
			if (f != null) {
				f.setName(funcname, SourceType.IMPORTED);
			}
			line = null;
			if(reader.hasNextLine()) {
				line = reader.nextLine().stripTrailing();
			}
			while (line != null && line.startsWith("\t\t")) {
				String[] varnode_loc = line.replace('(', ' ').replace(')', ' ').strip().split(",");
				String loc_aspc = varnode_loc[0].strip();
				String loc_offset = varnode_loc[1].strip();
				int loc_size = Integer.parseInt(varnode_loc[2].strip());

				AddressSpace aspc = currentProgram.getCompilerSpec().getAddressSpace(loc_aspc);
				Address addr = aspc.getAddress(loc_aspc + ":" + loc_offset);

				Variable var;
				if (loc_aspc.equals("register")) {
					var = new ParameterImpl(varname, Undefined.getUndefinedDataType(loc_size), addr, currentProgram);
				} else if (loc_aspc.equals("stack")) {
					var = new LocalVariableImpl(varname, 0, Undefined.getUndefinedDataType(loc_size), addr, currentProgram);
				} else {
					reader.close();
					throw new Exception("Unknown address space" + line);
				}
				try {
					if (f != null) {
						f.addLocalVariable(var, SourceType.IMPORTED);
					}
				} catch (DuplicateNameException e) {
					// Do nothing if it already exists
				}
				line = null;
				if(reader.hasNextLine()) {
					line = reader.nextLine().stripTrailing();
				}
			}
		}

		reader.close();
	}
}
