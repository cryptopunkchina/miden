use crate::{Example, init_log};
use log::{debug, info};
use miden::{Assembler, ProgramInputs};
use vm_core::program::blocks::CodeBlock;

// EXAMPLE BUILDER
// ================================================================================================

pub fn get_example(flag: usize) -> Example {
    // convert flag to a field element
    let flag = flag as u64;

    // determine the expected result
    let expected_result = match flag {
        0 => 15u64,
        1 => 37u64,
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
        push.2
        push.4
        storew.mem.10000
        dropw

        loadw.mem.10000
        drop
        drop
        swap
        push.2
        push.1
        if.true
            u32add
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

    let root = program.root() ;
    execute_code_block_print(root);

    Example {
        program,
        inputs: ProgramInputs::new(&[], &[flag].to_vec(), [].to_vec()).unwrap(),
        pub_inputs: vec![],
        expected_result: vec![expected_result],
        num_outputs: 1,
    }
}

fn execute_code_block_print(root: &CodeBlock) {
    match root {
        CodeBlock::Join(block) => {
            info!("join");
            info!("join code hash:{:?}", block.hash());
            execute_code_block_print(block.first());
            execute_code_block_print(block.second());
        },
        CodeBlock::Split(block) => {
            info!("Split");
            info!("split code hash:{:?}", block.hash());
            execute_code_block_print(block.on_true());
            execute_code_block_print(block.on_false());
        },
        CodeBlock::Loop(block) => {
            info!("Loop");
        },
        CodeBlock::Span(block) =>  {
            info!("Span");
            info!("span code hash:{:?}", block.hash());
            for (index, item) in block.op_batches().iter().enumerate() {

                info!("span code index:{:?}, ops:{:?}",index, item.ops());
                info!("span code num_groups:{:?}:{:?}",item.num_groups(), item.groups());
                info!("span code op_counter:{:?}",item.op_counts());
            }
        }
        CodeBlock::Proxy(_) => {

        },
        _ => {

        }
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
