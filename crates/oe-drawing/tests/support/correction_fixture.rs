pub fn fixture() -> Vec<u8> {
    let mut s = String::from(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1027\n9\n$HANDSEED\n5\nF0\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nBLOCK_RECORD\n5\n1\n70\n3\n",
    );
    for (h, n) in [("2", "*Model_Space"), ("3", "Cell"), ("4", "Assembly")] {
        s.push_str(&format!("0\nBLOCK_RECORD\n5\n{h}\n330\n1\n100\nAcDbSymbolTableRecord\n100\nAcDbBlockTableRecord\n2\n{n}\n70\n0\n"));
    }
    s.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n");
    for (h, n, owner, body, end) in [
        (
            "10",
            "Cell",
            "3",
            "0\nLINE\n5\n11\n330\n3\n8\n0\n10\n1\n20\n2\n11\n21\n21\n2\n0\nCIRCLE\n5\n12\n330\n3\n8\n0\n10\n11\n20\n7\n40\n2\n",
            "13",
        ),
        (
            "20",
            "Assembly",
            "4",
            "0\nINSERT\n5\n21\n330\n4\n8\n0\n2\nCell\n10\n0\n20\n0\n",
            "22",
        ),
    ] {
        s.push_str(&format!("0\nBLOCK\n5\n{h}\n330\n{owner}\n100\nAcDbEntity\n8\n0\n100\nAcDbBlockBegin\n2\n{n}\n70\n0\n10\n0\n20\n0\n30\n0\n3\n{n}\n{body}0\nENDBLK\n5\n{end}\n330\n{owner}\n8\n0\n"));
    }
    s.push_str("0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    for (h, x) in [("30", 0), ("31", 50)] {
        s.push_str(&format!(
            "0\nINSERT\n5\n{h}\n330\n2\n8\n0\n2\nAssembly\n10\n{x}\n20\n0\n"
        ));
    }
    s.push_str("0\nLINE\n5\n40\n330\n2\n8\n0\n10\n  3.000 \n20\n9\n11\n23\n21\n9\n999\nKeep this source spacing.\n0\nTEXT\n5\n41\n330\n2\n8\n0\n10\n7\n20\n14\n40\n3\n1\nQ\n0\nENDSEC\n0\nEOF\n");
    s.replace('\n', "\r\n").into_bytes()
}
