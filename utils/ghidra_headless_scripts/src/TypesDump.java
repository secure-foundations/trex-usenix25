// Used by TypesExporter and TypesRecoverer
//@author Jay Bosamiya

import java.io.BufferedWriter;
import java.io.FileWriter;
import java.util.ArrayDeque;
import java.util.Deque;
import java.util.HashSet;
import java.util.Set;

import ghidra.app.decompiler.DecompileResults;
import ghidra.app.decompiler.flatapi.FlatDecompilerAPI;
import ghidra.app.util.headless.HeadlessScript;
import ghidra.program.model.data.BitFieldDataType;
import ghidra.program.model.data.BuiltInDataType;
import ghidra.program.model.data.DataType;
import ghidra.program.model.data.DataTypeComponent;
import ghidra.program.model.data.Pointer;
import ghidra.program.model.data.Structure;
import ghidra.program.model.data.TypeDef;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Listing;
import ghidra.program.model.listing.Variable;
import ghidra.program.model.pcode.HighFunction;
import ghidra.program.model.pcode.HighSymbol;

public abstract class TypesDump extends HeadlessScript {
	protected void dump_types(String extension, boolean use_decompiler, String single_function) throws Exception {
		FlatDecompilerAPI decomp = null;
		if (use_decompiler) {
			decomp = new FlatDecompilerAPI(this);
			decomp.initialize();
		}
		String output_file_name = currentProgram.getName() + "." + extension;
		@SuppressWarnings("resource")
		BufferedWriter output_file = new BufferedWriter(new FileWriter(output_file_name));

		output_file.write("PROGRAM\n");
		output_file.write("name " + currentProgram.getName() + "\n");
		output_file.write("\n");

		Set<DataType> datatypes = new HashSet<DataType>();

		output_file.write("VARIABLE_TYPES\n");
		Listing listing = currentProgram.getListing();
		for (Function f : listing.getFunctions(true)) {
			if (f.isThunk()) {
				continue;
			}
			if (single_function != null) {
				if (!f.getName().contentEquals(single_function)) {
					continue;
				}
			}
			DecompileResults decompres = use_decompiler ? decomp.getDecompiler().decompileFunction(f, 30, monitor) : null;
			HighFunction hf = use_decompiler ? decompres.getHighFunction() : null;
			for (Variable var : f.getAllVariables()) {
				DataType typ = var.getDataType();
				if (hf != null && (var.getSymbol() != null)) {
					HighSymbol hs = hf.getSymbol(var.getSymbol().getID());
					if (hs != null) {
						typ = hs.getDataType();
					}
				}
				String varname = var.getName() + "@" + f.getName() + "@" + f.getEntryPoint();
				if (varname.contains("\t") || var.getName().contains("@") || f.getName().contains("@")) {
					throw new Exception("ERROR: Contains invalid char in variable name: " + varname);
				}
				output_file.write("\t" + varname + "\t" + typ.getName() + "\n");
				datatypes.add(typ);
			}
			// output_file.write("\n");
		}
		output_file.write("\n");

		Set<DataType> seen = new HashSet<DataType>();
		Deque<DataType> queue = new ArrayDeque<DataType>();
		queue.addAll(datatypes);

		output_file.write("TYPE_INFORMATION\n");
		while (!queue.isEmpty()) {
			DataType typ = queue.removeFirst();
			if (seen.contains(typ)) {
				continue;
			}
			seen.add(typ);

			// XXX: Composite and CompositeInternal are taken care of via Structure and
			// Union, afaict

			output_file.write("\t" + typ.getName() + "\n");

			if (typ instanceof ghidra.program.model.data.DefaultDataType) {
				// From docs: "Provides an implementation of a byte that has not been defined
				// yet as a particular type of data in the program."
				output_file.write("\t\tDefaultDataType\n");

			} else if (typ instanceof ghidra.program.model.data.Pointer) {
				DataType pointee = ((Pointer) typ).getDataType();
				output_file.write("\t\tPointer\t" + typ.getLength() + "\t" + pointee.getName() + "\n");
				queue.push(pointee);

			} else if (typ instanceof ghidra.program.model.data.Composite) {
				if (typ instanceof ghidra.program.model.data.Structure) {
					output_file.write("\t\tStructure\n");
				} else if (typ instanceof ghidra.program.model.data.Union) {
					output_file.write("\t\tUnion\n");
				} else {
					throw new Exception(
							"Unreachable: Handle type " + typ + " of class " + typ.getClass() + " of instance Union");
				}

				ghidra.program.model.data.Composite ctyp = (ghidra.program.model.data.Composite) typ;
				for (DataTypeComponent dtc : ctyp.getComponents()) {
					if (dtc.getDataType().isZeroLength() && dtc.getLength() != 0) {
						println("Got zero length " + dtc + " but length is non-zero");
						throw new Exception("Zero length non-zero length data type");
					}
					output_file.write("\t\t\t" + dtc.getOrdinal() + "\t" + dtc.getOffset() + "\t"
							+ dtc.getDataType().getName() + "\t" + dtc.getLength() + "\t" + dtc.getFieldName() + "\n");
					queue.push(dtc.getDataType());
				}

			} else if (typ instanceof ghidra.program.model.data.TypeDef) {
				DataType btyp = ((TypeDef) typ).getDataType();
				output_file.write("\t\tTypeDef\t" + btyp.getName() + "\n");
				queue.push(btyp);

			} else if (typ instanceof ghidra.program.model.data.Enum) {
				output_file.write("\t\tEnum\t" + typ.getLength() + "\n");
				ghidra.program.model.data.Enum etyp = (ghidra.program.model.data.Enum) typ;
				for (String nm : etyp.getNames()) {
					output_file.write("\t\t\t" + nm + "\t" + etyp.getValue(nm) + "\n");
				}

			} else if (typ instanceof ghidra.program.model.data.Array) {
				ghidra.program.model.data.Array atyp = (ghidra.program.model.data.Array)typ;
				DataType elemtyp = atyp.getDataType();
				output_file.write("\t\tArray\t" + elemtyp.getName() + "\t" + atyp.getElementLength() + "\t" + atyp.getNumElements() + "\n");
				queue.push(elemtyp);

			} else if (typ instanceof ghidra.program.model.data.FunctionDefinition) {
				output_file.write("\t\tFunctionDefinition\tCURRENTLY_EXPORT_UNIMPLEMENTED\n");

			} else if (typ instanceof ghidra.program.model.data.BuiltInDataType) {
				output_file.write("\t\tBuiltInDataType");
				String ctd = ((BuiltInDataType) typ).getCTypeDeclaration(typ.getDataOrganization());
				if (ctd != null) {
					output_file.write("\t\"" + ctd + "\"");
				}
				output_file.write("\n");

			} else if (typ instanceof ghidra.program.model.data.BitFieldDataType) {
				output_file.write("\t\tBitFieldDataType");
				BitFieldDataType bftyp = (BitFieldDataType)typ;
				queue.push(bftyp.getBaseDataType());
				output_file.write(""
						+ "\t" + bftyp.getBaseDataType().getName()
						+ "\t" + bftyp.getBitOffset()
						+ "\t" + bftyp.getBitSize()
						+ "\n");

			} else if (typ instanceof ghidra.program.model.data.ArrayStringable) {
				output_file.write("\t\tArrayStringable\n");
				throw new Exception(
						"TODO: Handle type " + typ + " of class " + typ.getClass() + " of instance ArrayStringable");

			} else if (typ instanceof ghidra.program.model.data.DataTypeWithCharset) {
				output_file.write("\t\tDataTypeWithCharset\n");
				throw new Exception("TODO: Handle type " + typ + " of class " + typ.getClass()
						+ " of instance DataTypeWithCharset");

			} else if (typ instanceof ghidra.program.model.data.Dynamic) {
				output_file.write("\t\tDynamic\n");
				throw new Exception(
						"TODO: Handle type " + typ + " of class " + typ.getClass() + " of instance Dynamic");

			} else if (typ instanceof ghidra.program.model.data.FactoryDataType) {
				output_file.write("\t\tFactoryDataType\n");
				throw new Exception(
						"TODO: Handle type " + typ + " of class " + typ.getClass() + " of instance FactoryDataType");

			} else if (typ instanceof ghidra.program.model.data.StructureInternal) {
				output_file.write("\t\tStructureInternal\n");
				throw new Exception(
						"TODO: Handle type " + typ + " of class " + typ.getClass() + " of instance StructureInternal");

			} else if (typ instanceof ghidra.program.model.data.UnionInternal) {
				output_file.write("\t\tUnionInternal\n");
				throw new Exception(
						"TODO: Handle type " + typ + " of class " + typ.getClass() + " of instance UnionInternal");

			} else {
				throw new Exception("Unreachable: unknown type " + typ + " of class " + typ.getClass()
						+ ". This should not happen unless the docs are lying to us.");
			}
		}
		output_file.write("\n");

		output_file.close();
		println("Done exporting to " + output_file_name);
	}
}
