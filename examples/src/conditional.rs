use crate::{Example, init_log};
use log::debug;
use miden::{Assembler, ProgramInputs};

// EXAMPLE BUILDER
// ================================================================================================

pub fn get_example(flag: usize) -> Example {
    // convert flag to a field element
    let flag = flag as u64;

    // determine the expected result
    let expected_result = match flag {
        0 => 15u64,
        1 => 35u64,
        _ => panic!("flag must be a binary value"),
    };

    // construct the program which either adds or multiplies two numbers
    // based on the value provided via secret inputs
    let assembler = Assembler::new();
    //storew.mem.0
    let program = assembler.compile_script(
        "
    begin
        push.3

        push.5
        pow2
        push.10
        push.11
        storew.mem.10000
        dropw

        loadw.mem.10000
        drop
        drop
        swap
        push.1
        if.true
            u32add
        else
            u32mul
        end
    end",
    )
    .unwrap();

    debug!(
        "Generated a program to test conditional execution; expected result: {}",
        expected_result
    );

    Example {
        program,
        inputs: ProgramInputs::new(&[], &[flag].to_vec(), [].to_vec()).unwrap(),
        pub_inputs: vec![],
        expected_result: vec![expected_result],
        num_outputs: 1,
    }
}

// EXAMPLE TESTER
// ================================================================================================

#[test]
fn test_conditional_example() {
    init_log("debug");
    let example = get_example(1);
    super::test_example(example, false);
}

#[test]
fn test_conditional_example_fail() {
    let example = get_example(1);
    super::test_example(example, true);
}
