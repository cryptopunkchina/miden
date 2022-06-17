use super::{
    range::AuxTraceHints as RangeCheckerAuxTraceHints, Digest, Felt, FieldElement, Process,
    StackTopState,
};
use core::slice;
use log::info;
use vm_core::{StarkField, MIN_STACK_DEPTH, MIN_TRACE_LEN, STACK_TRACE_OFFSET, TRACE_WIDTH};
use winterfell::{EvaluationFrame, Matrix, Serializable, Trace, TraceLayout};

mod range;

// CONSTANTS
// ================================================================================================

/// Number of rows at the end of an execution trace which are injected with random values.
const NUM_RAND_ROWS: usize = 1;

// TYPE ALIASES
// ================================================================================================

type RandomCoin = vm_core::utils::RandomCoin<Felt, vm_core::hasher::Hasher>;

// VM EXECUTION TRACE
// ================================================================================================

pub struct AuxTraceHints {
    range: RangeCheckerAuxTraceHints,
}

/// TODO: for now this consists only of system register trace, stack trace, range check trace, and
/// auxiliary table trace, but will also need to include the decoder trace.
pub struct ExecutionTrace {
    pub meta: Vec<u8>,
    layout: TraceLayout,
    pub main_trace: Matrix<Felt>,
    aux_trace_hints: AuxTraceHints,
    // TODO: program hash should be retrieved from decoder trace, but for now we store it explicitly
    program_hash: Digest,
}

impl ExecutionTrace {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// Number of rows at the end of an execution trace which are injected with random values.
    pub const NUM_RAND_ROWS: usize = NUM_RAND_ROWS;

    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------
    /// Builds an execution trace for the provided process.
    pub(super) fn new(process: Process, program_hash: Digest) -> Self {
        // use program hash to initialize random element generator; this generator will be used
        // to inject random values at the end of the trace; using program hash here is OK because
        // we are using random values only to stabilize constraint degree, and not to achieve
        // perfect zero knowledge.
        let rng = RandomCoin::new(&program_hash.to_bytes());
        let (main_trace, aux_trace_hints) = finalize_trace(process, rng);

        Self {
            meta: Vec::new(),
            layout: TraceLayout::new(TRACE_WIDTH, [2], [1]),
            main_trace: Matrix::new(main_trace),
            aux_trace_hints,
            program_hash,
        }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// TODO: add docs
    pub fn program_hash(&self) -> Digest {
        // TODO: program hash should be read from the decoder trace
        self.program_hash
    }

    /// Returns the initial state of the top 16 stack registers.
    pub fn init_stack_state(&self) -> StackTopState {
        let mut result = [Felt::ZERO; MIN_STACK_DEPTH];
        for (i, result) in result.iter_mut().enumerate() {
            *result = self.main_trace.get_column(i + STACK_TRACE_OFFSET)[0];
        }
        result
    }

    /// Returns the final state of the top 16 stack registers.
    pub fn last_stack_state(&self) -> StackTopState {
        let last_step = self.length() - NUM_RAND_ROWS - 1;
        let mut result = [Felt::ZERO; MIN_STACK_DEPTH];
        for (i, result) in result.iter_mut().enumerate() {
            *result = self.main_trace.get_column(i + STACK_TRACE_OFFSET)[last_step];
        }
        result
    }

    // TEST HELPERS
    // --------------------------------------------------------------------------------------------

    #[allow(dead_code)]
    pub fn print(&self) {
        let mut row = [Felt::ZERO; TRACE_WIDTH];
        for i in 0..self.length() {
            self.main_trace.read_row_into(i, &mut row);
            println!("{:?}", row.iter().map(|v| v.as_int()).collect::<Vec<_>>());
        }
    }

    #[cfg(test)]
    pub fn test_finalize_trace(process: Process) -> (Vec<Vec<Felt>>, AuxTraceHints) {
        let rng = RandomCoin::new(&[0; 32]);
        finalize_trace(process, rng)
    }
}

// TRACE TRAIT IMPLEMENTATION
// ================================================================================================

impl Trace for ExecutionTrace {
    type BaseField = Felt;

    fn layout(&self) -> &TraceLayout {
        &self.layout
    }

    fn length(&self) -> usize {
        self.main_trace.num_rows()
    }

    fn meta(&self) -> &[u8] {
        &self.meta
    }

    fn main_segment(&self) -> &Matrix<Felt> {
        &self.main_trace
    }

    fn build_aux_segment<E: FieldElement<BaseField = Felt>>(
        &mut self,
        aux_segments: &[Matrix<E>],
        rand_elements: &[E],
    ) -> Option<Matrix<E>> {
        // We only have one auxiliary segment.
        if !aux_segments.is_empty() {
            return None;
        }

        // Add the range checker's running product columns.
        let mut aux_columns = range::build_aux_columns(
            self.length(),
            &self.aux_trace_hints.range,
            rand_elements,
            self.main_trace.get_column(range::V_COL_IDX),
        );

        // inject random values into the last rows of the trace
        let mut rng = RandomCoin::new(&self.program_hash.to_bytes());
        for i in self.length() - NUM_RAND_ROWS..self.length() {
            for column in aux_columns.iter_mut() {
                column[i] = rng.draw().expect("failed to draw a random value");
            }
        }

        Some(Matrix::new(aux_columns))
    }

    fn read_main_frame(&self, row_idx: usize, frame: &mut EvaluationFrame<Felt>) {
        let next_row_idx = (row_idx + 1) % self.length();
        self.main_trace.read_row_into(row_idx, frame.current_mut());
        self.main_trace
            .read_row_into(next_row_idx, frame.next_mut());
    }
}

// TRACE FRAGMENT
// ================================================================================================

/// TODO: add docs
pub struct TraceFragment<'a> {
    pub data: Vec<&'a mut [Felt]>,
}

impl<'a> TraceFragment<'a> {
    /// Creates a new TraceFragment with its data allocated to the specified capacity.
    pub fn new(capacity: usize) -> Self {
        TraceFragment {
            data: Vec::with_capacity(capacity),
        }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the number of columns in this execution trace fragment.
    pub fn width(&self) -> usize {
        self.data.len()
    }

    /// Returns the number of rows in this execution trace fragment.
    pub fn len(&self) -> usize {
        self.data[0].len()
    }

    // DATA MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Updates a single cell in this fragment with provided value.
    #[inline(always)]
    pub fn set(&mut self, row_idx: usize, col_idx: usize, value: Felt) {
        self.data[col_idx][row_idx] = value;
    }

    /// Returns a mutable iterator the the columns of this fragment.
    pub fn columns(&mut self) -> slice::IterMut<'_, &'a mut [Felt]> {
        self.data.iter_mut()
    }

    /// Adds a new column to this fragment by pushing a mutable slice with the first `len`
    /// elements of the provided column. Returns the rest of the provided column as a separate
    /// mutable slice.
    pub fn push_column_slice(&mut self, column: &'a mut [Felt], len: usize) -> &'a mut [Felt] {
        let (column_fragment, rest) = column.split_at_mut(len);
        self.data.push(column_fragment);
        rest
    }

    // TEST METHODS
    // --------------------------------------------------------------------------------------------

    #[cfg(test)]
    pub fn trace_to_fragment(trace: &'a mut [Vec<Felt>]) -> Self {
        let mut data = Vec::new();
        for column in trace.iter_mut() {
            data.push(column.as_mut_slice());
        }
        Self { data }
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Converts a process into a set of execution trace columns for each component of the trace.
///
/// The process includes:
/// - Determining the length of the trace required to accommodate the longest trace column.
/// - Padding the columns to make sure all columns are of the same length.
/// - Inserting random values in the last row of all columns. This helps ensure that there
///   are no repeating patterns in each column and each column contains a least two distinct
///   values. This, in turn, ensures that polynomial degrees of all columns are stable.
fn finalize_trace(process: Process, mut rng: RandomCoin) -> (Vec<Vec<Felt>>, AuxTraceHints) {
    let (system, decoder, stack, range, aux_table) = process.to_components();

    let clk = system.clk();

    // trace lengths of system and stack components must be equal to the number of executed cycles
    assert_eq!(clk, system.trace_len(), "inconsistent system trace lengths");
    // TODO: check decoder trace length
    assert_eq!(clk, stack.trace_len(), "inconsistent stack trace lengths");

    // Get the trace length required to hold all execution trace steps.
    let max_len = [clk, range.trace_len(), aux_table.trace_len()]
        .into_iter()
        .max()
        .expect("failed to get max of component trace lengths");
    info!("range len:{}, aux_table len:{}, aux_table hash len:{}", range.trace_len(), aux_table.trace_len(), aux_table.hasher.trace_len());
    // pad the trace length to the next power of two and ensure that there is space for the
    // rows to hold random values
    let trace_len = (max_len + NUM_RAND_ROWS).next_power_of_two();
    assert!(
        trace_len >= MIN_TRACE_LEN,
        "trace length must be at least {}, but was {}",
        MIN_TRACE_LEN,
        trace_len
    );

    // combine all trace segments into the main trace
    let system_trace = system.into_trace(trace_len, NUM_RAND_ROWS);

    for aux in decoder.block_stack.blocks.iter().enumerate() {
        info!("trace BlockInfo index:{}, {:?}", aux.0, aux.1);
    }

    info!("trace span_context :{:?}", decoder.span_context);

    for aux in decoder.trace.op_idx_trace.iter().enumerate() {
        info!("trace op_idx_trace index:{}, {:?}", aux.0, aux.1);
    }

    for aux in decoder.trace.addr_trace.iter().enumerate() {
        info!("trace addr_trace index:{}, {:?}", aux.0, aux.1);
    }

    for aux in decoder.trace.op_bits_trace.iter().enumerate() {
        info!("trace op_bits_trace index:{}, len:{},op: {:?}", aux.0,aux.1.len(), aux.1);
    }

    for aux in decoder.trace.group_count_trace.iter().enumerate() {
        info!("trace group_count_trace index:{}, {:?}", aux.0, aux.1);
    }

    for aux in decoder.trace.hasher_trace.iter().enumerate() {
        info!("trace hasher_trace index:{}, {:?}", aux.0, aux.1);
    }

    for aux in decoder.trace.in_span_trace.iter().enumerate() {
        info!("trace in_span_trace index:{}, {:?}", aux.0, aux.1);
    }

    let mut decoder_trace = decoder.into_trace(trace_len, NUM_RAND_ROWS);
    for aux in decoder_trace.iter().enumerate() {
        info!("trace decoder_trace index:{}, {:?}", aux.0, aux.1);
    }
    let stack_trace = stack.into_trace(trace_len, NUM_RAND_ROWS);
    let range_check_trace = range.into_trace(trace_len, NUM_RAND_ROWS);

    let mut aux_table_trace = aux_table.into_trace(trace_len, NUM_RAND_ROWS);
    for aux in aux_table_trace.iter_mut().enumerate() {
        // info!("trace aux_table_trace index:{}, {:?}", aux.0, aux.1);
    }
    let mut trace = system_trace
        .into_iter()
        .chain(decoder_trace)
        .chain(stack_trace)
        .chain(range_check_trace.trace)
        .chain(aux_table_trace)
        .collect::<Vec<_>>();

    for (index,item) in trace.iter().enumerate() {
        info!("trace index:{},item:{:?}", index, item.len());
    }
    // inject random values into the last rows of the trace
    for i in trace_len - NUM_RAND_ROWS..trace_len {
        for column in trace.iter_mut() {
            column[i] = rng.draw().expect("failed to draw a random value");
        }
    }

    let aux_trace_hints = AuxTraceHints {
        range: range_check_trace.aux_trace_hints,
    };

    (trace, aux_trace_hints)
}
