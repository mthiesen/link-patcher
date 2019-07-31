use super::Patch;
use crate::exe_tools::Architecture;
use byteorder::{ByteOrder, LittleEndian};
use capstone::{prelude::*, Insn};
use common_failures::prelude::*;
use failure::{bail, err_msg};
use itertools::Itertools;
use lazy_static::lazy_static;
use std::ops::Range;

// -------------------------------------------------------------------------------------------------

const LOOK_BACK_BUFFER: usize = 15;
const LOOK_AHEAD_BUFFER: usize = 100;
const MAX_MAGIC_DISTANCE: usize = 1024;
const DANS_MAGIC_BYTES: [u8; 4] = [0x44, 0x61, 0x6E, 0x53];
const RICH_MAGIC_BYTES: [u8; 4] = [0x52, 0x69, 0x63, 0x68];
const XOR_EAX_EAX: &[u8] = &[0x33, 0xC0];

// -------------------------------------------------------------------------------------------------

pub fn find_candidate_ranges<'a>(code: &'a [u8]) -> impl Iterator<Item = Range<usize>> + 'a {
    code.windows(4)
        .enumerate()
        .filter(|(_, bytes)| bytes == &DANS_MAGIC_BYTES || bytes == &RICH_MAGIC_BYTES)
        .tuple_windows::<(_, _)>()
        .filter_map(
            move |((first_pos, first_magic), (second_pos, second_magic))| {
                if first_magic != second_magic && second_pos - first_pos <= MAX_MAGIC_DISTANCE {
                    Some(Range {
                        start: first_pos,
                        end: second_pos + 4
                    })
                } else {
                    None
                }
            }
        )
}

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod test_find_candidate_ranges {
    use super::*;

    fn insert_dummy_bytes(v: &mut Vec<u8>, n: usize) {
        let mut next_byte = 0u8;
        for _ in 0..n {
            v.push(next_byte);
            next_byte = (next_byte + 1) % 32;
        }
    }

    #[test]
    fn dans_first() {
        let mut data = Vec::new();
        insert_dummy_bytes(&mut data, 100);
        data.extend_from_slice(&DANS_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 80);
        data.extend_from_slice(&RICH_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 800);

        let result = find_candidate_ranges(&data).collect::<Vec<_>>();
        let expected = &[Range {
            start: 100,
            end: 188
        }];

        assert_eq!(expected, &result[..]);
    }

    #[test]
    fn rich_first() {
        let mut data = Vec::new();
        insert_dummy_bytes(&mut data, 100);
        data.extend_from_slice(&RICH_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 80);
        data.extend_from_slice(&DANS_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 800);

        let result = find_candidate_ranges(&data).collect::<Vec<_>>();
        let expected = &[Range {
            start: 100,
            end: 188
        }];

        assert_eq!(expected, &result[..]);
    }

    #[test]
    fn multiple_pairs() {
        let mut data = Vec::new();
        insert_dummy_bytes(&mut data, 100);
        data.extend_from_slice(&RICH_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 80);
        data.extend_from_slice(&DANS_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 800);
        data.extend_from_slice(&RICH_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 50);
        data.extend_from_slice(&RICH_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 80);

        let result = find_candidate_ranges(&data).collect::<Vec<_>>();
        let expected = &[
            Range {
                start: 100,
                end: 188
            },
            Range {
                start: 184,
                end: 992
            }
        ];

        assert_eq!(expected, &result[..]);
    }

    #[test]
    fn only_one_magic() {
        let mut data = Vec::new();
        insert_dummy_bytes(&mut data, 100);
        data.extend_from_slice(&RICH_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 80);

        let result = find_candidate_ranges(&data).collect::<Vec<_>>();
        let expected: &[Range<usize>] = &[];

        assert_eq!(expected, &result[..]);
    }

    #[test]
    fn magic_at_beginning() {
        let mut data = Vec::new();
        data.extend_from_slice(&RICH_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 80);
        data.extend_from_slice(&DANS_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 800);

        let result = find_candidate_ranges(&data).collect::<Vec<_>>();
        let expected = &[Range { start: 0, end: 88 }];

        assert_eq!(expected, &result[..]);
    }

    #[test]
    fn magic_at_end() {
        let mut data = Vec::new();
        insert_dummy_bytes(&mut data, 100);
        data.extend_from_slice(&RICH_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 80);
        data.extend_from_slice(&DANS_MAGIC_BYTES);

        let result = find_candidate_ranges(&data).collect::<Vec<_>>();
        let expected = &[Range {
            start: 100,
            end: 188
        }];

        assert_eq!(expected, &result[..]);
    }

    #[test]
    fn magic_near_end() {
        let mut data = Vec::new();
        insert_dummy_bytes(&mut data, 100);
        data.extend_from_slice(&RICH_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 80);
        data.extend_from_slice(&DANS_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 10);

        let result = find_candidate_ranges(&data).collect::<Vec<_>>();
        let expected = &[Range {
            start: 100,
            end: 188
        }];

        assert_eq!(expected, &result[..]);
    }

    #[test]
    fn too_far_apart() {
        let mut data = Vec::new();
        insert_dummy_bytes(&mut data, 100);
        data.extend_from_slice(&DANS_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 2000);
        data.extend_from_slice(&RICH_MAGIC_BYTES);
        insert_dummy_bytes(&mut data, 800);

        let result = find_candidate_ranges(&data).collect::<Vec<_>>();
        let expected: &[Range<usize>] = &[];

        assert_eq!(expected, &result[..]);
    }
}

// -------------------------------------------------------------------------------------------------

fn gen_disassemble_ranges(
    code: &[u8],
    candidate_range: Range<usize>
) -> impl Iterator<Item = Range<usize>> {
    let start_range = Range {
        start: candidate_range.start.saturating_sub(LOOK_BACK_BUFFER),
        end: candidate_range.start
    };
    use std::cmp::min;
    let end = min(candidate_range.end + LOOK_AHEAD_BUFFER, code.len());

    start_range.map(move |start| Range { start, end })
}

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod test_gen_disassemble_ranges {
    use super::*;

    #[test]
    fn generates_all_ranges() {
        let dummy_code = vec![0u8; 1000];

        let candidate_range = Range {
            start: 500,
            end: 600
        };
        let result: Vec<_> = gen_disassemble_ranges(&dummy_code, candidate_range).collect();
        assert_eq!(LOOK_BACK_BUFFER, result.len());
        assert_eq!(500 - LOOK_BACK_BUFFER, result[0].start);
        assert_eq!(499, result.last().unwrap().start);
        assert!(result
            .iter()
            .all(|Range { end, .. }| *end == 600 + LOOK_AHEAD_BUFFER));
    }

    #[test]
    fn clamp_start() {
        let dummy = vec![0u8; 1000];

        let candidate_range = Range { start: 5, end: 100 };
        let result: Vec<_> = gen_disassemble_ranges(&dummy, candidate_range).collect();
        let end = 100 + LOOK_AHEAD_BUFFER;
        assert_eq!(
            vec![
                Range { start: 0, end },
                Range { start: 1, end },
                Range { start: 2, end },
                Range { start: 3, end },
                Range { start: 4, end },
            ],
            result
        );

        assert_eq!(
            0,
            gen_disassemble_ranges(&dummy, Range { start: 0, end: 100 }).count()
        );
    }

    #[test]
    fn clamp_end() {
        let dummy = vec![0u8; 1000];

        let candidate_range = Range {
            start: 900,
            end: 998
        };
        let result: Vec<_> = gen_disassemble_ranges(&dummy, candidate_range).collect();
        assert_eq!(LOOK_BACK_BUFFER, result.len());
        assert!(result.iter().all(|Range { end, .. }| *end == 1000));
    }
}

// -------------------------------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum InstructionType {
    UseDansMagic,
    UseRichMagic,
    ModifyEax,
    Ret,
    Other
}

// -------------------------------------------------------------------------------------------------

fn classify_instruction(instruction: &Insn) -> InstructionType {
    lazy_static! {
        static ref DANS_MAGIC_SUFFIX: String =
            format!(", 0x{:08x}", LittleEndian::read_u32(&DANS_MAGIC_BYTES));
        static ref RICH_MAGIC_SUFFIX: String =
            format!(", 0x{:08x}", LittleEndian::read_u32(&RICH_MAGIC_BYTES));
    }

    if instruction.mnemonic().map_or(false, |m| m == "ret") {
        InstructionType::Ret
    } else if let Some(op_str) = instruction.op_str() {
        if op_str.ends_with(DANS_MAGIC_SUFFIX.as_str()) {
            InstructionType::UseDansMagic
        } else if op_str.ends_with(RICH_MAGIC_SUFFIX.as_str()) {
            InstructionType::UseRichMagic
        } else if op_str.starts_with("eax, ") {
            InstructionType::ModifyEax
        } else {
            InstructionType::Other
        }
    } else {
        InstructionType::Other
    }
}

// -------------------------------------------------------------------------------------------------

pub(crate) fn find_patch(
    arch: Architecture,
    code_section_offset: u64,
    code: &[u8]
) -> Result<Patch> {
    let capstone_architecture = match arch {
        Architecture::X86 => arch::x86::ArchMode::Mode32,
        Architecture::X64 => arch::x86::ArchMode::Mode64
    };

    let mut cs = Capstone::new()
        .x86()
        .mode(capstone_architecture)
        .syntax(arch::x86::ArchSyntax::Intel)
        .detail(true)
        .build()
        .context("Failed to create Capstone instance.")?;

    for range in find_candidate_ranges(code).flat_map(|range| gen_disassemble_ranges(code, range)) {
        let code_block = &code[range.clone()];
        if let Ok(instructions) = cs.disasm_all(code_block, 0) {
            let mut filtered_instructions = instructions
                .iter()
                .filter_map(|instruction| {
                    let instruction_type = classify_instruction(&instruction);
                    if instruction_type != InstructionType::Other {
                        Some((instruction, instruction_type))
                    } else {
                        None
                    }
                })
                .peekable();

            let instructions: Vec<_> = filtered_instructions
                .peeking_take_while(|&(_, instruction_type)| {
                    instruction_type != InstructionType::Ret
                })
                .collect();

            // The next instruction must be the "ret" that stopped the peeking_take_while.
            // Otherwise the instruction sequence did not end with a "ret" and therefore must be
            // rejected.
            let next_instruction_type = filtered_instructions
                .next()
                .map_or(InstructionType::Other, |(_, instruction_type)| {
                    instruction_type
                });
            if next_instruction_type != InstructionType::Ret {
                continue;
            }

            // The instruction sequence must make use of both magics.
            let uses_dans_magic = instructions
                .iter()
                .any(|&(_, instruction_type)| instruction_type == InstructionType::UseDansMagic);
            let uses_rich_magic = instructions
                .iter()
                .any(|&(_, instruction_type)| instruction_type == InstructionType::UseRichMagic);
            if !uses_dans_magic || !uses_rich_magic {
                continue;
            }

            // We patch the last instruction in the function that modifies EAX.
            if let Some((instruction_to_patch, _)) = instructions
                .iter()
                .rev()
                .find(|(_, instruction_type)| *instruction_type == InstructionType::ModifyEax)
            {
                if instruction_to_patch.bytes().len() < XOR_EAX_EAX.len() {
                    bail!("Cannot create patch. Instruction is too short.");
                }

                let original_code = {
                    let start = range.start + instruction_to_patch.address() as usize;
                    let end = start + instruction_to_patch.bytes().len();
                    &code[start..end]
                };

                if original_code == XOR_EAX_EAX {
                    bail!("Cannot create patch. Is seems like the code is already patched.");
                }

                let patched_code = {
                    let mut patched_code = Vec::from(XOR_EAX_EAX);
                    while patched_code.len() < original_code.len() {
                        patched_code.push(0x90);
                    }
                    patched_code
                };

                return Ok(Patch {
                    offset: code_section_offset
                        + range.start as u64
                        + instruction_to_patch.address(),
                    original_code: original_code.to_vec(),
                    patched_code
                });
            }
        }
    }

    Err(err_msg("Unable to find code to patch."))
}

// -------------------------------------------------------------------------------------------------

#[cfg(test)]
mod test_find_patch {
    use super::*;

    const USE_DANS_MAGIC: &[u8] = &[0x81, 0xE2, 0x44, 0x61, 0x6E, 0x53];
    const USE_RICH_MAGIC: &[u8] = &[0xC7, 0x06, 0x52, 0x69, 0x63, 0x68];
    const MOV_EAX_EDI: &[u8] = &[0x8B, 0xC7];
    const RET: &[u8] = &[0xC3];

    fn insert_dummy_instructions(v: &mut Vec<u8>, mut n: usize) {
        const SINGLE_BYTE: [u8; 5] = [0x90, 0x41, 0xaa, 0xa4, 0x5D];
        const MULTI_BYTE: [&[u8]; 4] = [
            &[0x85, 0xF1],
            &[0x8D, 0x0C, 0x89],
            &[0x84, 0xC0],
            &[0x3B, 0xFA]
        ];

        let mut single_byte_iter = SINGLE_BYTE.iter().cycle();
        let mut multi_byte_iter = MULTI_BYTE.iter().cycle();
        while n > 0 {
            let next_multi_byte = multi_byte_iter.next().unwrap();
            if next_multi_byte.len() <= n {
                v.extend_from_slice(next_multi_byte);
                n -= next_multi_byte.len();
            } else {
                v.push(*single_byte_iter.next().unwrap());
                n -= 1;
            }
        }
    }

    #[test]
    fn insert_dummy_instructions_correct_len() {
        let mut v = Vec::new();
        insert_dummy_instructions(&mut v, 1234);
        assert_eq!(1234, v.len());

        let mut v = vec![90u8; 50];
        insert_dummy_instructions(&mut v, 100);
        assert_eq!(150, v.len());
    }

    #[test]
    fn valid_patch() {
        let mut instructions = Vec::new();
        insert_dummy_instructions(&mut instructions, 10);
        instructions.extend_from_slice(USE_DANS_MAGIC);
        insert_dummy_instructions(&mut instructions, 50);
        instructions.extend_from_slice(USE_RICH_MAGIC);
        insert_dummy_instructions(&mut instructions, 10);
        instructions.extend_from_slice(MOV_EAX_EDI);
        insert_dummy_instructions(&mut instructions, 40);
        instructions.extend_from_slice(RET);

        let expected = Patch {
            offset: 1082,
            original_code: MOV_EAX_EDI.into(),
            patched_code: XOR_EAX_EAX.into()
        };

        assert_eq!(
            expected,
            find_patch(Architecture::X86, 1000, instructions.as_slice()).unwrap()
        );
        assert_eq!(
            expected,
            find_patch(Architecture::X64, 1000, instructions.as_slice()).unwrap()
        );
    }

    #[test]
    fn already_patched() {
        let mut instructions = Vec::new();
        insert_dummy_instructions(&mut instructions, 10);
        instructions.extend_from_slice(USE_DANS_MAGIC);
        insert_dummy_instructions(&mut instructions, 50);
        instructions.extend_from_slice(USE_RICH_MAGIC);
        insert_dummy_instructions(&mut instructions, 10);
        instructions.extend_from_slice(XOR_EAX_EAX);
        insert_dummy_instructions(&mut instructions, 40);
        instructions.extend_from_slice(RET);

        assert!(find_patch(Architecture::X86, 1000, instructions.as_slice()).is_err());
        assert!(find_patch(Architecture::X64, 1000, instructions.as_slice()).is_err());
    }

    #[test]
    fn missing_magic() {
        let mut instructions = Vec::new();
        insert_dummy_instructions(&mut instructions, 10);
        instructions.extend_from_slice(USE_DANS_MAGIC);
        insert_dummy_instructions(&mut instructions, 60);
        instructions.extend_from_slice(MOV_EAX_EDI);
        insert_dummy_instructions(&mut instructions, 40);
        instructions.extend_from_slice(RET);

        assert!(find_patch(Architecture::X86, 1000, instructions.as_slice()).is_err());
        assert!(find_patch(Architecture::X64, 1000, instructions.as_slice()).is_err());
    }

    #[test]
    fn missing_eax_modification() {
        let mut instructions = Vec::new();
        instructions.extend_from_slice(USE_DANS_MAGIC);
        insert_dummy_instructions(&mut instructions, 50);
        instructions.extend_from_slice(USE_RICH_MAGIC);
        insert_dummy_instructions(&mut instructions, 50);

        assert!(find_patch(Architecture::X86, 1000, instructions.as_slice()).is_err());
        assert!(find_patch(Architecture::X64, 1000, instructions.as_slice()).is_err());
    }

    #[test]
    fn missing_ret() {
        let mut instructions = Vec::new();
        instructions.extend_from_slice(USE_DANS_MAGIC);
        insert_dummy_instructions(&mut instructions, 50);
        instructions.extend_from_slice(USE_RICH_MAGIC);
        insert_dummy_instructions(&mut instructions, 10);
        instructions.extend_from_slice(MOV_EAX_EDI);
        insert_dummy_instructions(&mut instructions, 40);

        assert!(find_patch(Architecture::X86, 1000, instructions.as_slice()).is_err());
        assert!(find_patch(Architecture::X64, 1000, instructions.as_slice()).is_err());
    }
}
