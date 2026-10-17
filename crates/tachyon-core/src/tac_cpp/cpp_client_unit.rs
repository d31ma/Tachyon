use super::{
    CppAttribute, CppFailure, CppFile, CppFolder, CppLowering, CppMarkerCheck, CppMember,
    CppMemberKind, CppProgram, CppTestCase, CppTools,
};
use crate::Failure;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub(crate) struct CppClientUnit {
    name: String,
    setup: String,
    cases: Vec<CppTestCase>,
    fakes: Vec<(String, usize)>,
    fresh: bool,
    lowered: String,
    path: PathBuf,
}

impl CppClientUnit {
    const TESTS: &'static str = "client/tests";
    const SOURCE: &'static str = "unit.cpp";
    const BINARY: &'static str = "unit";
    const FLAGS: [&'static str; 3] = ["-std=c++20", "-O0", "-fno-color-diagnostics"];
    const FAILED: u16 = 2021;
    const CONFIGURE: &'static str = "configure";
    const FRESH_START: &'static str = "FreshStart";
    const NO_TEST: &'static str =
        "A C++ test unit declares one class marked [[Test(<Name>TestSetup)]]";
    const NO_SETUP: &'static str = "[[Test]] names its TestSetup class first, for example [[Test(TallyTestSetup)]], and client/tests/TallyTestSetup.hpp declares class [[TestSetup]] TallyTestSetup";
    const FAKE_FORM: &'static str = "A C++ [[Fake]] field holds the stand-in object itself, for example `[[Fake]] GreetingClient greetingClient;`: no reference, no pointer, no static";
    const FAKE_TYPE: &'static str = "A C++ [[Fake]] field stands in for a C++ service in client/services or a C++ [[HTTPClient]] in client/remotes";
    const FAKE_IN_SETUP: &'static str = "A C++ [[Fake]] field belongs in the [[Test]] class, where construct<T>() gives it to the service; the TestSetup class cannot hold one";

    pub(crate) fn run(project: &Path, relative: &Path, source: &str) -> Result<String, Failure> {
        let unit = Self::read(relative, source)?;
        let (setup, answers, fresh) = Self::setup(project, &unit)?;
        let program = CppProgram::tested(project)?;
        unit.check_fake_types(&program)?;
        let located = CppProgram::located(&unit.path, &unit.lowered);
        let units = format!("{setup}{located}{}", unit.main(answers, fresh));
        Self::compiled_and_run(relative, &program.test_source(&units))
    }

    fn read(relative: &Path, source: &str) -> Result<Self, Failure> {
        let file = CppFile::read(relative, source)?;
        CppMarkerCheck::check(relative, &file.class)?;
        let marker = file
            .class
            .markers
            .iter()
            .find(|marker| marker.name == "Test")
            .ok_or_else(|| CppFailure::unreadable(relative, file.class.name_at, Self::NO_TEST))?;
        let setup = Self::setup_name(relative, marker)?;
        let fakes = Self::fakes(relative, &file.class.members)?;
        let fake_types = fakes
            .iter()
            .map(|member| (member.plain_type(), member.name_at))
            .collect();
        Ok(Self {
            name: file.class.name.clone(),
            setup,
            cases: CppTestCase::read_all(relative, &file.class)?,
            fakes: fake_types,
            fresh: Self::starts_fresh(&file.class),
            lowered: CppLowering::test_unit(&file, source, &fakes),
            path: relative.to_path_buf(),
        })
    }

    fn fakes<'a>(relative: &Path, members: &'a [CppMember]) -> Result<Vec<&'a CppMember>, Failure> {
        let fakes: Vec<&CppMember> = members
            .iter()
            .filter(|member| member.marked("Fake"))
            .collect();
        for fake in &fakes {
            let plain =
                [fake.is_instance(CppMemberKind::Field), !fake.is_reference()] == [true, true];
            plain
                .then_some(())
                .ok_or_else(|| CppFailure::unreadable(relative, fake.name_at, Self::FAKE_FORM))?;
        }
        Ok(fakes)
    }

    fn check_fake_types(&self, program: &CppProgram) -> Result<(), Failure> {
        for (type_name, at) in &self.fakes {
            program
                .has_class(type_name)
                .then_some(())
                .ok_or_else(|| CppFailure::pending(&self.path, *at, Self::FAKE_TYPE))?;
        }
        Ok(())
    }

    fn setup_name(relative: &Path, marker: &CppAttribute) -> Result<String, Failure> {
        marker
            .arguments
            .split(',')
            .next()
            .map(str::trim)
            .filter(|name| [name.is_empty(), name.starts_with('{')] == [false, false])
            .map(str::to_owned)
            .ok_or_else(|| CppFailure::unreadable(relative, marker.at, Self::NO_SETUP))
    }

    fn starts_fresh(class: &super::CppClass) -> bool {
        class
            .markers
            .iter()
            .any(|marker| marker.name == Self::FRESH_START)
    }

    fn setup(project: &Path, unit: &Self) -> Result<(String, bool, bool), Failure> {
        let path = project
            .join(Self::TESTS)
            .join(format!("{}.hpp", unit.setup));
        let (relative, source) = CppFolder::read(project, &path)
            .map_err(|_| CppFailure::unreadable(&unit.path, 0, Self::NO_SETUP))?;
        let file = CppFile::read(&relative, &source)?;
        CppMarkerCheck::check(&relative, &file.class)?;
        let marked = file
            .class
            .markers
            .iter()
            .any(|marker| marker.name == "TestSetup");
        let valid = [marked, file.class.name == unit.setup] == [true, true];
        valid
            .then_some(())
            .ok_or_else(|| CppFailure::unreadable(&relative, file.class.name_at, Self::NO_SETUP))?;
        let fake = file
            .class
            .members
            .iter()
            .find(|member| member.marked("Fake"));
        fake.map_or(Ok(()), |fake| {
            Err(CppFailure::unreadable(
                &relative,
                fake.name_at,
                Self::FAKE_IN_SETUP,
            ))
        })?;
        let answers = file.class.members.iter().any(Self::is_configure);
        let lowered = CppLowering::model(&file, &source);
        let fresh = [unit.fresh, Self::starts_fresh(&file.class)].contains(&true);
        Ok((CppProgram::located(&relative, &lowered), answers, fresh))
    }

    fn is_configure(member: &CppMember) -> bool {
        [
            member.kind == CppMemberKind::Method,
            member.is_static,
            member.name == Self::CONFIGURE,
        ] == [true, true, true]
    }

    fn prepared(&self, answers: bool) -> String {
        if !answers {
            return String::new();
        }
        format!(
            "tachyon::TacTestAnswers answers({}::{}()); ",
            self.setup,
            Self::CONFIGURE
        )
    }

    fn main(&self, answers: bool, fresh: bool) -> String {
        let started = ["", "tachyon::TacFreshStart::next(); "][usize::from(fresh)];
        let prepared = format!("{started}{}", self.prepared(answers));
        let mut cases = String::new();
        for case in &self.cases {
            let _ = writeln!(
                cases,
                "    runner.run(\"{}\", [] {{ {prepared}{} unit; tachyon::TacTestCase::finish([&unit] {{ return unit.{}; }}); }});",
                case.name, self.name, case.call
            );
        }
        format!(
            "#line 1 \"tachyon-test-main.cpp\"\nint main() {{\n    tachyon::TacTestRunner runner(\"{}\");\n{cases}    std::puts(runner.text().c_str());\n    return 0;\n}}\n",
            self.name
        )
    }

    fn compiled_and_run(relative: &Path, text: &str) -> Result<String, Failure> {
        let staged = tempfile::Builder::new()
            .prefix("tachyon-cpp-unit-")
            .tempdir()
            .map_err(|error| Self::failure(relative, &error.to_string()))?;
        std::fs::write(staged.path().join(Self::SOURCE), text)
            .map_err(|error| Self::failure(relative, &error.to_string()))?;
        let compiled = Command::new(CppTools::test_compiler())
            .args(Self::FLAGS)
            .args([Self::SOURCE, "-o", Self::BINARY])
            .current_dir(staged.path())
            .output()
            .map_err(|error| {
                Self::failure(
                    relative,
                    &format!("cannot start the C++ compiler: {error}. Run ty doctor"),
                )
            })?;
        Self::succeeded(relative, &compiled)?;
        let ran = Command::new(staged.path().join(Self::BINARY))
            .env("TAC_TEST_UNIT", "1")
            .output()
            .map_err(|error| Self::failure(relative, &error.to_string()))?;
        Self::succeeded(relative, &ran)?;
        Ok(String::from_utf8_lossy(&ran.stdout).into_owned())
    }

    fn succeeded(relative: &Path, output: &Output) -> Result<(), Failure> {
        output
            .status
            .success()
            .then_some(())
            .ok_or_else(|| Self::failure(relative, &String::from_utf8_lossy(&output.stderr)))
    }

    fn failure(relative: &Path, detail: &str) -> Failure {
        let message = format!("The C++ test unit did not run: {}", detail.trim());
        CppFailure::coded(Self::FAILED, relative, 0, &message)
    }
}
